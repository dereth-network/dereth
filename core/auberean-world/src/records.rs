//! The world's records, read on demand from the data files the host opened, decoded once and kept.
//!
//! The host hands over the containers ([`auberean_dat::DatFile`], which read through the host's own
//! source); this module only decides which file holds an id and decodes it. A client hands over
//! its data cache's reading too ([`WorldRecords::read_cells_through`]): the cell file's records
//! (landblocks, their infos, their light, the interior cells) are then read through it, so a
//! record the server sent and the cache stored is read as the file's own, and one neither holds
//! is missing until it comes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use auberean_assets::entity::Entity;
use auberean_assets::fx::FxTable;
use auberean_assets::landblock::{Cell, Landblock, LandblockInfo, LightInfo};
use auberean_assets::region::{
    DayDesc, FogDesc, GameTime, Landscape, Region, SceneDesc, SkyDesc, SurfaceDesc, TerrainDesc,
};
use auberean_assets::setup::Setup;
use auberean_assets::tables::IdMap;
use auberean_assets::visual::Scene;
use auberean_assets::visual::{Appearance, VisualDesc};
use auberean_assets::{decode, Context, Decode};
use auberean_dat::{DatFile, RecordType};
use auberean_primitives::DataId;

use crate::land::{LandConstants, LandError};
use crate::look::LookRecords;
use crate::place::{LoadedSetup, PlaceRecords};
use crate::property::BlockProperties;
use crate::scenery::SceneSource;
use crate::terrain::Surfaces;
use crate::water::WaterSet;

/// The region record every world starts from.
pub const REGION_ID: u32 = 0x0F00_0000;

/// How a client reads the cell file's records: a record's bytes by its id and its type, through
/// its data cache.
pub type CellReader = Arc<dyn Fn(u32, u32) -> Option<Vec<u8>> + Send + Sync>;

/// Whether a record is a picture list or a picture made for one language: the upper parts of their
/// id ranges, `0x40100000` and `0x41100000` up.
const fn is_language_picture(id: u32) -> bool {
    matches!(id >> 24, 0x40 | 0x41) && id & 0x00F0_0000 != 0
}

/// A world's data files and its decoded records.
pub struct WorldRecords {
    portal: DatFile,
    cell: DatFile,
    highres: Option<DatFile>,
    /// The language's file, for the pictures made for one language (a signpost's words).
    language: OnceLock<DatFile>,
    ctx: Context,
    visuals: Mutex<HashMap<u32, Option<Arc<VisualDesc>>>>,
    setups: Mutex<HashMap<u32, Option<Arc<LoadedSetup>>>>,
    entities: Mutex<HashMap<u32, Option<Arc<Entity>>>>,
    scenes: Mutex<HashMap<u32, Option<Arc<Scene>>>>,
    appearances: Mutex<HashMap<u32, Option<Arc<Appearance>>>>,
    fx_tables: Mutex<HashMap<u32, Option<Arc<FxTable>>>>,
    /// Whether each mesh asked about is weighted to bones.
    weighted: Mutex<HashMap<u32, bool>>,
    /// The client's reading of the cell file's records, when it gave one.
    cells: RwLock<Option<CellReader>>,
}

impl std::fmt::Debug for WorldRecords {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldRecords")
            .field("highres", &self.highres.is_some())
            .finish_non_exhaustive()
    }
}

/// The region and the descriptions the landscape is drawn from, let go of whole with the world.
// D2-07474
#[derive(Clone, Debug)]
pub struct RegionSet {
    pub region: Region,
    pub land: LandConstants,
    pub terrain: TerrainDesc,
    pub surface: SurfaceDesc,
    pub sky: Option<SkyDesc>,
    pub calendar: Option<GameTime>,
    pub fog: Option<FogDesc>,
    /// The scenery kinds and their scenes.
    pub scenes: Option<SceneDesc>,
    /// The water and its block maps.
    pub water: Option<WaterSet>,
    /// The fog kinds' block map.
    pub fog_kinds: Option<auberean_assets::region::BlockMap>,
    /// The properties laid over the world, and the block map of each that has one.
    pub properties: Option<Arc<BlockProperties>>,
}

impl RegionSet {
    /// Whether a body may walk on block `(x, y)`: the world's passability map; a world without one
    /// lets it.
    #[must_use]
    pub fn passable(&self, x: u32, y: u32) -> bool {
        self.properties.as_ref().is_none_or(|p| p.passable(x, y))
    }

    /// The surface descriptions, as the terrain builder takes them.
    #[must_use]
    pub fn surfaces(&self) -> Surfaces<'_> {
        Surfaces {
            terrain: &self.terrain,
            surface: &self.surface,
        }
    }
}

/// Why a world could not be read.
#[derive(Debug)]
pub enum WorldError {
    /// A record the world needs is missing or did not decode.
    Record(u32, String),
    Land(LandError),
}

impl WorldRecords {
    /// A world over its portal and cell files (and the high-resolution pictures, if the install has
    /// them), with the install's decoding context.
    #[must_use]
    pub fn new(portal: DatFile, cell: DatFile, highres: Option<DatFile>, ctx: Context) -> Self {
        Self {
            portal,
            cell,
            highres,
            language: OnceLock::new(),
            ctx,
            visuals: Mutex::default(),
            setups: Mutex::default(),
            entities: Mutex::default(),
            scenes: Mutex::default(),
            appearances: Mutex::default(),
            fx_tables: Mutex::default(),
            weighted: Mutex::default(),
            cells: RwLock::new(None),
        }
    }

    /// The ids the portal file holds from `lo` to `hi`.
    pub fn portal_ids(&self, lo: u32, hi: u32) -> impl Iterator<Item = u32> + '_ {
        self.portal.entries_in(lo, hi).map(|e| e.id)
    }

    /// The language's file: a picture made for one language is read from it from now on. The first
    /// given stays.
    pub fn use_language_file(&self, file: DatFile) {
        let _ = self.language.set(file);
    }

    /// Read the cell file's records through `reader` from now on (`None`: from the file).
    pub fn read_cells_through(&self, reader: Option<CellReader>) {
        match self.cells.write() {
            Ok(mut r) => *r = reader,
            Err(e) => *e.into_inner() = reader,
        }
    }

    /// The bytes of cell-file record `id` of type `kind`: through the client's reader when it gave
    /// one, else from the file.
    fn cell_bytes(&self, id: u32, kind: RecordType) -> Option<Vec<u8>> {
        let reader = match self.cells.read() {
            Ok(r) => r.clone(),
            Err(e) => e.into_inner().clone(),
        };
        match reader {
            Some(r) => r(id, kind.0),
            None => self.cell.read(id).ok(),
        }
    }

    /// The bytes of portal record `id`: a picture made for one language from the language's file;
    /// from the high-resolution file first for other pictures, else the portal file. Landblocks
    /// and cells are read by their own methods, since their ids overlap the portal's.
    #[must_use]
    pub fn bytes(&self, id: u32) -> Option<Vec<u8>> {
        if is_language_picture(id) {
            return self.language.get()?.read(id).ok();
        }
        if (id >> 24) == 0x41 {
            if let Some(Ok(b)) = self.highres.as_ref().map(|h| h.read(id)) {
                return Some(b);
            }
        }
        self.portal.read(id).ok()
    }

    /// Whether the data holds portal record `id`, read from the files' directories alone.
    #[must_use]
    pub fn holds(&self, id: u32) -> bool {
        if is_language_picture(id) {
            return self.language.get().is_some_and(|l| l.entry(id).is_some());
        }
        ((id >> 24) == 0x41 && self.highres.as_ref().is_some_and(|h| h.entry(id).is_some()))
            || self.portal.entry(id).is_some()
    }

    /// Effect table `id`, read once: the bodies' effects look in it every frame.
    #[must_use]
    pub fn fx_table(&self, id: u32) -> Option<Arc<FxTable>> {
        let mut tables = self
            .fx_tables
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        tables
            .entry(id)
            .or_insert_with(|| self.get::<FxTable>(id).map(Arc::new))
            .clone()
    }

    /// Record `id` decoded as `T`, or `None` when it is missing or does not decode.
    #[must_use]
    pub fn get<T: Decode>(&self, id: u32) -> Option<T> {
        decode::<T>(id, &self.bytes(id)?, &self.ctx).ok()
    }

    /// The record an enumeration value maps to (0 for none): the portal file's master id map
    /// names the mapping's id map, and that map names the record.
    #[must_use]
    pub fn data_id_by_enum(&self, value: u32, mapping: u32) -> u32 {
        let lookup = |m: &IdMap, k: u32| {
            m.ids
                .iter()
                .chain(m.tool_ids.iter())
                .find(|(x, _)| *x == k)
                .map_or(0, |(_, d)| d.0)
        };
        let Some(master) = self.get::<IdMap>(self.portal.header().master_map_id) else {
            return 0;
        };
        let sub = lookup(&master, mapping);
        self.get::<IdMap>(sub).map_or(0, |m| lookup(&m, value))
    }

    fn need<T: Decode>(&self, id: u32) -> Result<T, WorldError> {
        let bytes = self
            .bytes(id)
            .ok_or_else(|| WorldError::Record(id, "missing".into()))?;
        decode::<T>(id, &bytes, &self.ctx).map_err(|e| WorldError::Record(id, e.to_string()))
    }

    /// The region and its landscape descriptions.
    ///
    /// # Errors
    /// When the region, the landscape constants, the terrain or the surface description is missing
    /// or does not decode, or the constants are out of range.
    // D2-07497, D2-07498, D2-07473
    pub fn region(&self) -> Result<RegionSet, WorldError> {
        let region: Region = self.need(REGION_ID)?;
        // The region's current record of a kind: always the first of its list.
        // D2-07481, D2-07482, D2-07483, D2-07485, D2-07486, D2-07493
        let first =
            |list: &[auberean_assets::region::IndexedRecord]| list.first().map(|r| r.record.0);
        let landscape: Landscape = self.need(region.landscape.0)?;
        let land = LandConstants::from_record(&landscape).map_err(WorldError::Land)?;
        let terrain = self.need(first(&region.terrains).unwrap_or(0))?;
        let surface = self.need(first(&region.surfaces).unwrap_or(0))?;
        let sky: Option<SkyDesc> = first(&region.skies).and_then(|id| self.get(id));
        // D2-13059
        let calendar = sky.as_ref().and_then(|s| self.get(s.game_time.0));
        let fog = first(&region.fogs).and_then(|id| self.get(id));
        let scenes = first(&region.scenes).and_then(|id| self.get(id));
        // D2-03549
        let fog_kinds = fog
            .as_ref()
            .and_then(|f: &FogDesc| self.get(f.distance_map.0));
        let water = first(&region.waters)
            .and_then(|id| self.get::<auberean_assets::region::WaterDesc>(id))
            .and_then(|desc| {
                Some(WaterSet {
                    heights: self.get(desc.height_map.0)?,
                    kinds: self.get(desc.kind_map.0)?,
                    desc,
                })
            });
        let properties = self.properties_of(&region).map(Arc::new);
        Ok(RegionSet {
            properties,
            region,
            land,
            terrain,
            surface,
            sky,
            calendar,
            fog,
            scenes,
            water,
            fog_kinds,
        })
    }

    /// The properties the region lays over the world, read on their own.
    #[must_use]
    pub fn block_properties(&self) -> Option<BlockProperties> {
        self.properties_of(&self.get::<Region>(REGION_ID)?)
    }

    /// `region`'s current property description and its block maps.
    // D2-07487
    fn properties_of(&self, region: &Region) -> Option<BlockProperties> {
        let id = region.properties.first()?.record.0;
        let desc = self.get::<auberean_assets::region::PropertyDesc>(id)?;
        let maps = desc
            .properties
            .iter()
            .map(|p| self.get(p.block_map.0))
            .collect();
        Some(BlockProperties { desc, maps })
    }

    /// The landblock at block coordinates `(x, y)`.
    #[must_use]
    pub fn landblock(&self, x: i32, y: i32) -> Option<Landblock> {
        let id = block_id(x, y)? | 0xFFFF;
        decode::<Landblock>(id, &self.cell_bytes(id, RecordType::LANDBLOCK)?, &self.ctx).ok()
    }

    /// The landblock info (placed objects) of block `(x, y)`, if it has one.
    // D2-03622
    #[must_use]
    pub fn landblock_info(&self, x: i32, y: i32) -> Option<LandblockInfo> {
        let id = block_id(x, y)? | 0xFFFE;
        let bytes = self.cell_bytes(id, RecordType::LANDBLOCK_INFO)?;
        decode::<LandblockInfo>(id, &bytes, &self.ctx).ok()
    }

    /// The light record of block `(x, y)`, if it has one.
    #[must_use]
    pub fn light_info(&self, x: i32, y: i32) -> Option<LightInfo> {
        let id = block_id(x, y)? | 0xFFFC;
        decode::<LightInfo>(id, &self.cell_bytes(id, RecordType::LIGHT_INFO)?, &self.ctx).ok()
    }

    /// Interior cell `low` (`0x0100` and up) of block `(x, y)`.
    #[must_use]
    pub fn cell(&self, x: i32, y: i32, low: u16) -> Option<Cell> {
        let id = block_id(x, y)? | u32::from(low);
        decode::<Cell>(id, &self.cell_bytes(id, RecordType::CELL)?, &self.ctx).ok()
    }

    /// A sky day.
    #[must_use]
    pub fn day(&self, id: DataId) -> Option<DayDesc> {
        self.get(id.0)
    }

    /// Every landblock id the cell file holds, as block coordinates.
    #[must_use]
    pub fn landblock_coordinates(&self) -> Vec<(i32, i32)> {
        self.cell
            .entries()
            .filter(|e| e.id & 0xFFFF == 0xFFFF)
            .map(|e| {
                (
                    i32::from(e.id.to_be_bytes()[0]),
                    i32::from(e.id.to_be_bytes()[1]),
                )
            })
            .collect()
    }

    /// Setup `id` as loaded: the same one every time, kept with the records, so what is changed in
    /// it reaches everything that reads the setup ([`LoadedSetup`]).
    #[must_use]
    pub fn loaded_setup(&self, id: DataId) -> Option<Arc<LoadedSetup>> {
        if let Ok(m) = self.setups.lock() {
            if let Some(v) = m.get(&id.0) {
                return v.clone();
            }
        }
        let v = self
            .get::<Setup>(id.0)
            .map(|s| Arc::new(LoadedSetup::new(Arc::new(s))));
        match self.setups.lock() {
            // Two readers that load it at once share the one kept first.
            Ok(mut m) => m.entry(id.0).or_insert(v).clone(),
            Err(_) => v,
        }
    }

    fn cached<T: Decode>(
        &self,
        map: &Mutex<HashMap<u32, Option<Arc<T>>>>,
        id: DataId,
    ) -> Option<Arc<T>> {
        if let Ok(m) = map.lock() {
            if let Some(v) = m.get(&id.0) {
                return v.clone();
            }
        }
        let v = self.get::<T>(id.0).map(Arc::new);
        if let Ok(mut m) = map.lock() {
            m.insert(id.0, v.clone());
        }
        v
    }
}

/// The id of block `(x, y)`'s landblock with a zero low word, if both coordinates are bytes.
#[must_use]
pub fn block_id(x: i32, y: i32) -> Option<u32> {
    let (x, y) = (u8::try_from(x).ok()?, u8::try_from(y).ok()?);
    Some((u32::from(x) << 24) | (u32::from(y) << 16))
}

impl SceneSource for WorldRecords {
    fn scene(&self, id: DataId) -> Option<Arc<Scene>> {
        self.cached(&self.scenes, id)
    }
}

impl LookRecords for WorldRecords {
    fn visual(&self, id: DataId) -> Option<Arc<VisualDesc>> {
        self.cached(&self.visuals, id)
    }

    fn appearance(&self, id: DataId) -> Option<Arc<Appearance>> {
        self.cached(&self.appearances, id)
    }
}

impl PlaceRecords for WorldRecords {
    /// A mesh's kind: the word after its id.
    fn mesh_kind(&self, id: DataId) -> Option<u32> {
        let b = self.portal.read_start(id.0).ok()?;
        Some(u32::from_le_bytes(b.get(4..8)?.try_into().ok()?))
    }

    /// Whether a mesh is weighted to bones: its vertex format word, after its id, its kind and its
    /// counted materials, names matrices. Kept once asked.
    // D2-05477
    fn mesh_weighted(&self, id: DataId) -> bool {
        let format = |b: &[u8]| {
            let word = |at: usize| Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?));
            let materials = usize::try_from(word(8)?).ok()?;
            word(materials.checked_mul(4)?.checked_add(12)?)
        };
        // A mesh's first block holds the word, unless it counts more materials than fit.
        let read = || {
            let format = format(&self.portal.read_start(id.0).ok()?)
                .or_else(|| format(&self.bytes(id.0)?))?;
            Some(auberean_assets::mesh::matrix_count(format) != 0)
        };
        let lock = || match self.weighted.lock() {
            Ok(m) => m,
            Err(e) => e.into_inner(),
        };
        if let Some(w) = lock().get(&id.0) {
            return *w;
        }
        let w = read().unwrap_or(false);
        lock().insert(id.0, w);
        w
    }

    /// The setup as it is now ([`WorldRecords::loaded_setup`]).
    fn setup(&self, id: DataId) -> Option<Arc<Setup>> {
        self.loaded_setup(id).map(|s| s.get())
    }

    fn entity(&self, id: DataId) -> Option<Arc<Entity>> {
        self.cached(&self.entities, id)
    }
}
