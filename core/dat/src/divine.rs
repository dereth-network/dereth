//! DataID-to-type mapping and type-to-dat routing.
//!
//! This is a table of ranges, **not** `id >> 24`. `0x0E00xxxx` resolves per id,
//! `0x06`/`0x07` are one type, `0x23`/`0x24` are one type, `0x78`–`0x7F` are one type, and the cell
//! dat carries no type in the id at all.
//!
//! The engine type table has 50 types and the game table has 15; see
//! `docs/formats/02-file-ids-and-types.md`.
//! The game map tries the game ranges first and then chains to
//! the engine map, which is why [`divine_type`] searches the game
//! table first.

use dereth_primitives::{DataId, DataType};

/// Which container a type is routed to. The cache classifies with the portal, cell and local
/// type tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DatKind {
    /// `client_portal.dat`, and `client_highres.dat`, which shares the portal id space.
    Portal,
    /// `client_cell_<region>.dat`.
    Cell,
    /// `client_local_<Language>.dat`.
    Local,
    /// Server-only types that are never packed into a shipped client dat.
    None,
}

/// One `DB_TYPE_*` constant. All 65 registered types, with the client's own values.
///
/// The values are the `DB_TYPE_*` constants, not the top byte of an id: `GFXOBJ` is `0x06` and its
/// ids start `0x01`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
#[allow(clippy::upper_case_acronyms)]
pub enum DbType {
    LandBlock = 0x1,
    Lbi = 0x2,
    Cell = 0x3,
    Lbo = 0x4,
    Instantiation = 0x5,
    GfxObj = 0x6,
    Setup = 0x7,
    Anim = 0x8,
    AnimationHook = 0x9,
    Palette = 0xA,
    SurfaceTexture = 0xB,
    RenderSurface = 0xC,
    Surface = 0xD,
    MTable = 0xE,
    Wave = 0xF,
    Environment = 0x10,
    ChatPoseTable = 0x11,
    ObjectHierarchy = 0x12,
    BadData = 0x13,
    TabooTable = 0x14,
    File2IdTable = 0x15,
    NameFilterTable = 0x16,
    MonitoredProperties = 0x17,
    PalSet = 0x18,
    Clothing = 0x19,
    DegradeInfo = 0x1A,
    Scene = 0x1B,
    Region = 0x1C,
    Keymap = 0x1D,
    RenderTexture = 0x1E,
    RenderMaterial = 0x1F,
    MaterialModifier = 0x20,
    MaterialInstance = 0x21,
    STable = 0x22,
    UiLayout = 0x23,
    EnumMapper = 0x24,
    StringTable = 0x25,
    DidMapper = 0x26,
    ActionMap = 0x27,
    DualDidMapper = 0x28,
    StringType = 0x29,
    ParticleEmitter = 0x2A,
    PhysicsScript = 0x2B,
    PhysicsScriptTable = 0x2C,
    MasterProperty = 0x2D,
    Font = 0x2E,
    FontLocal = 0x2F,
    StringState = 0x30,
    DbProperties = 0x31,
    RenderMesh = 0x43,
    WeenieDef = 0x1000_0001,
    CharGen = 0x1000_0002,
    Attribute2ndTable = 0x1000_0003,
    SkillTable = 0x1000_0004,
    SpellTable = 0x1000_0005,
    SpellComponentTable = 0x1000_0006,
    WTreasureSystem = 0x1000_0007,
    WCraftTable = 0x1000_0008,
    XpTable = 0x1000_0009,
    QuestDefDb = 0x1000_000A,
    GameEventDb = 0x1000_000B,
    QualityFilter = 0x1000_000C,
    CombatTable = 0x1000_000D,
    MutateFilter = 0x1000_000E,
    ContractTable = 0x1000_0010,
}

impl DbType {
    /// The `DB_TYPE_*` numeric value.
    #[must_use]
    pub fn value(self) -> u32 {
        self as u32
    }

    /// Categorized payloads carry a second dword after the DataID.
    /// Exactly three types.
    #[must_use]
    pub fn is_categorized(self) -> bool {
        matches!(
            self,
            Self::SurfaceTexture | Self::RenderSurface | Self::RenderTexture
        )
    }

    /// Which container this type is routed to.
    #[must_use]
    pub fn dat(self) -> DatKind {
        match self {
            Self::LandBlock | Self::Lbi | Self::Cell => DatKind::Cell,
            Self::UiLayout | Self::StringTable | Self::FontLocal | Self::StringState => {
                DatKind::Local
            }
            // The eight server-only types have no client-side file at all.
            Self::Lbo
            | Self::Instantiation
            | Self::AnimationHook
            | Self::WeenieDef
            | Self::WTreasureSystem
            | Self::WCraftTable
            | Self::GameEventDb
            | Self::MutateFilter => DatKind::None,
            _ => DatKind::Portal,
        }
    }

    /// The name the corresponding [`dereth_primitives::DataType`] seam variant has, when the shared
    /// vocabulary names this type.
    ///
    /// `DataType` is deliberately partial (`#[non_exhaustive]`, with an `Other(u8)` escape) and is
    /// a frozen shared contract; the complete 65-type catalogue lives here, in the dat crate. The
    /// seam only ever names the types other crates consume.
    #[must_use]
    pub fn seam_type(self) -> Option<DataType> {
        Some(match self {
            Self::GfxObj => DataType::GfxObj,
            Self::Setup => DataType::Setup,
            Self::Anim => DataType::Animation,
            Self::Palette => DataType::Palette,
            Self::SurfaceTexture => DataType::SurfaceTexture,
            // ACE's `Texture.cs` and the type catalogue both put RENDERSURFACE here.
            Self::RenderSurface => DataType::Texture,
            Self::Surface => DataType::Surface,
            Self::MTable => DataType::MotionTable,
            Self::Wave => DataType::Wave,
            Self::Environment => DataType::Environment,
            Self::PalSet => DataType::PaletteSet,
            Self::Clothing => DataType::ClothingTable,
            Self::DegradeInfo => DataType::DegradeInfo,
            Self::Scene => DataType::Scene,
            Self::Region => DataType::Region,
            Self::STable => DataType::SoundTable,
            Self::ParticleEmitter => DataType::ParticleEmitter,
            Self::PhysicsScript => DataType::PhysicsScript,
            Self::PhysicsScriptTable => DataType::PhysicsScriptTable,
            Self::LandBlock => DataType::Landblock,
            Self::Lbi => DataType::LandblockInfo,
            Self::Cell => DataType::EnvCell,
            _ => return None,
        })
    }
}

/// One registered `(base id, top id, type)` row. Ranges are inclusive.
type Row = (u32, u32, DbType);

/// The 15 game types, searched first.
///
/// Order is registration order — the client registers them in ascending `DB_TYPE_*` order —
/// which is what resolves any overlap.
const GAME_RANGES: &[Row] = &[
    (0x0000_0001, 0x0000_FFFF, DbType::WeenieDef),
    (0x0E00_0002, 0x0E00_0002, DbType::CharGen),
    (0x0E00_0003, 0x0E00_0003, DbType::Attribute2ndTable),
    (0x0E00_0004, 0x0E00_0004, DbType::SkillTable),
    (0x0E00_000E, 0x0E00_000E, DbType::SpellTable),
    (0x0E00_000F, 0x0E00_000F, DbType::SpellComponentTable),
    (0x0E00_0011, 0x0E00_0011, DbType::WTreasureSystem),
    (0x0E00_0019, 0x0E00_0019, DbType::WCraftTable),
    (0x0E00_0018, 0x0E00_0018, DbType::XpTable),
    (0x0E00_001B, 0x0E00_001B, DbType::QuestDefDb),
    (0x0E00_001C, 0x0E00_001C, DbType::GameEventDb),
    (0x0E01_0000, 0x0E01_FFFF, DbType::QualityFilter),
    (0x3000_0000, 0x3000_FFFF, DbType::CombatTable),
    (0x3800_0000, 0x3800_FFFF, DbType::MutateFilter),
    (0x0E00_001D, 0x0E00_001D, DbType::ContractTable),
];

/// The 50 engine types, in registration order.
///
/// The six types with no id space of their own (`LAND_BLOCK`, `LBI`, `CELL`, `LBO`,
/// `INSTANTIATION`, `ANIMATION_HOOK`) are registered with a base and top id of 0 and so are
/// absent here: `DivineType` can never return them. The cell types are routed by container, not by
/// id — see [`classify_cell_id`].
///
/// `FONT_LOCAL` and `STRING_STATE` genuinely overlap. Registration order puts
/// `FONT_LOCAL` (0x2F) first, so `0x40001000`–`0x40FFFFFF` resolves to `FONT_LOCAL` and
/// `STRING_STATE` is reachable only at `0x41000000`+ — exactly where language information was
/// placed. `STRING_STATE` therefore has the effective range
/// `0x41000000`–`0x41FFFFFF` after the overlap is resolved; the
/// registered base is `0x40001000`.
const ENGINE_RANGES: &[Row] = &[
    (0x0100_0000, 0x0100_FFFF, DbType::GfxObj),
    (0x0200_0000, 0x0200_FFFF, DbType::Setup),
    (0x0300_0000, 0x0300_FFFF, DbType::Anim),
    (0x0400_0000, 0x0400_FFFF, DbType::Palette),
    (0x0500_0000, 0x05FF_FFFF, DbType::SurfaceTexture),
    (0x0600_0000, 0x07FF_FFFF, DbType::RenderSurface),
    (0x0800_0000, 0x0800_FFFF, DbType::Surface),
    (0x0900_0000, 0x0900_FFFF, DbType::MTable),
    (0x0A00_0000, 0x0A00_FFFF, DbType::Wave),
    (0x0D00_0000, 0x0D00_FFFF, DbType::Environment),
    (0x0E00_0007, 0x0E00_0007, DbType::ChatPoseTable),
    (0x0E00_000D, 0x0E00_000D, DbType::ObjectHierarchy),
    (0x0E00_001A, 0x0E00_001A, DbType::BadData),
    (0x0E00_001E, 0x0E00_001E, DbType::TabooTable),
    (0x0E00_001F, 0x0E00_001F, DbType::File2IdTable),
    (0x0E00_0020, 0x0E00_0020, DbType::NameFilterTable),
    (0x0E02_0000, 0x0E02_FFFF, DbType::MonitoredProperties),
    (0x0F00_0000, 0x0F00_FFFF, DbType::PalSet),
    (0x1000_0000, 0x1000_FFFF, DbType::Clothing),
    (0x1100_0000, 0x1100_FFFF, DbType::DegradeInfo),
    (0x1200_0000, 0x1200_FFFF, DbType::Scene),
    (0x1300_0000, 0x1300_FFFF, DbType::Region),
    (0x1400_0000, 0x1400_FFFF, DbType::Keymap),
    (0x1500_0000, 0x15FF_FFFF, DbType::RenderTexture),
    (0x1600_0000, 0x16FF_FFFF, DbType::RenderMaterial),
    (0x1700_0000, 0x17FF_FFFF, DbType::MaterialModifier),
    (0x1800_0000, 0x18FF_FFFF, DbType::MaterialInstance),
    (0x1900_0000, 0x19FF_FFFF, DbType::RenderMesh),
    (0x2000_0000, 0x2000_FFFF, DbType::STable),
    (0x2100_0000, 0x21FF_FFFF, DbType::UiLayout),
    (0x2200_0000, 0x22FF_FFFF, DbType::EnumMapper),
    (0x2300_0000, 0x24FF_FFFF, DbType::StringTable),
    (0x2500_0000, 0x25FF_FFFF, DbType::DidMapper),
    (0x2600_0000, 0x2600_FFFF, DbType::ActionMap),
    (0x2700_0000, 0x27FF_FFFF, DbType::DualDidMapper),
    (0x3100_0000, 0x3100_FFFF, DbType::StringType),
    (0x3200_0000, 0x3200_FFFF, DbType::ParticleEmitter),
    (0x3300_0000, 0x3300_FFFF, DbType::PhysicsScript),
    (0x3400_0000, 0x3400_FFFF, DbType::PhysicsScriptTable),
    (0x3900_0000, 0x39FF_FFFF, DbType::MasterProperty),
    (0x4000_0000, 0x4000_0FFF, DbType::Font),
    (0x4000_1000, 0x40FF_FFFF, DbType::FontLocal),
    // UNVERIFIED: the registered base of STRING_STATE is 0x40001000, which
    // overlaps FONT_LOCAL entirely. Registration order (FONT_LOCAL first) makes it unreachable
    // below 0x41000000, and no `0x400010xx` file ships, so the resolution is untestable against
    // real data. Reproduced verbatim rather than "cleaned" into a disjoint table.
    (0x4000_1000, 0x41FF_FFFF, DbType::StringState),
    (0x7800_0000, 0x7FFF_FFFF, DbType::DbProperties),
];

/// The type a `DataID` divines to, or `None` for the ranges the id space leaves unused.
///
/// Game ranges are searched first, exactly as the client's game map does; within a table the
/// search is in registration order so that an overlap resolves the way the client's does.
#[must_use]
pub fn divine_type(id: DataId) -> Option<DbType> {
    let v = id.raw();
    if v == 0 {
        return None; // INVALID_DID is rejected by every load path.
    }
    GAME_RANGES
        .iter()
        .chain(ENGINE_RANGES)
        .find(|(base, top, _)| v >= *base && v <= *top)
        .map(|(_, _, t)| *t)
}

/// Which dat a type is routed to.
#[must_use]
pub fn dat_for_type(kind: DbType) -> DatKind {
    kind.dat()
}

/// The cell dat has its own id space, so `DivineType` is never consulted: the top 16 bits are
/// the landblock and the low 16 the cell index, and the container decides the type.
///
/// Returns `None` for the iteration list, for the 64 outdoor land cells, which are generated and
/// never stored, and for the indices no cell has: `0x0000` and `0x0041` to `0x00FF`, which the
/// client treats as invalid cell ids and which no cell dat stores. Indoor cells start at `0x0100`.
#[must_use]
pub fn classify_cell_id(id: DataId) -> Option<DbType> {
    let v = id.raw();
    if v == ITERATION_LIST.raw() {
        return None;
    }
    match v & 0xFFFF {
        0xFFFF => Some(DbType::LandBlock),
        0xFFFE => Some(DbType::Lbi),
        0x0001..=0x0040 => None, // generated outdoor land cells, never on disk
        0x0000 | 0x0041..=0x00FF => None, // not a cell index at all
        _ => Some(DbType::Cell),
    }
}

/// The iteration list's data id. Present in all four dats, has no type, and is read before any
/// type machinery exists.
pub const ITERATION_LIST: DataId = DataId(0xFFFF_0001);

#[cfg(test)]
mod tests {
    use super::*;

    /// A cell index that no cell has -- zero, or between the outdoor cells and the first indoor
    /// one -- names no record type; the neighbours on either side still do.
    #[test]
    fn a_cell_index_the_client_calls_invalid_classifies_to_nothing() {
        for low in [0x0000u32, 0x0041, 0x0080, 0x00FF] {
            assert_eq!(
                classify_cell_id(DataId(0xA9B4_0000 | low)),
                None,
                "{low:#06x}"
            );
        }
        assert_eq!(
            classify_cell_id(DataId(0xA9B4_0040)),
            None,
            "an outdoor cell"
        );
        assert_eq!(classify_cell_id(DataId(0xA9B4_0100)), Some(DbType::Cell));
        assert_eq!(classify_cell_id(DataId(0xA9B4_FFFE)), Some(DbType::Lbi));
        assert_eq!(
            classify_cell_id(DataId(0xA9B4_FFFF)),
            Some(DbType::LandBlock)
        );
    }

    /// Oracle: the client's two type-table initialisers.
    #[test]
    fn the_registered_table_has_sixty_five_types() {
        // 50 engine + 15 game; six engine types have no id range and so are not in the tables.
        assert_eq!(GAME_RANGES.len(), 15);
        assert_eq!(ENGINE_RANGES.len(), 44);
        assert_eq!(ENGINE_RANGES.len() + 6, 50);
    }

    /// Contract 9.8: the four irregular cases the spec calls out by name.
    #[test]
    fn divine_type_is_not_the_top_byte() {
        // 0x0E00xxxx resolves per id, and has holes.
        assert_eq!(divine_type(DataId(0x0E00_0002)), Some(DbType::CharGen));
        assert_eq!(
            divine_type(DataId(0x0E00_0007)),
            Some(DbType::ChatPoseTable)
        );
        assert_eq!(divine_type(DataId(0x0E00_000E)), Some(DbType::SpellTable));
        assert_eq!(divine_type(DataId(0x0E00_0000)), None);
        assert_eq!(divine_type(DataId(0x0E00_0005)), None);
        assert_eq!(divine_type(DataId(0x0E00_0021)), None);
        // 0x06 and 0x07 are one type.
        assert_eq!(
            divine_type(DataId(0x0600_0133)),
            Some(DbType::RenderSurface)
        );
        assert_eq!(
            divine_type(DataId(0x07FF_FFFF)),
            Some(DbType::RenderSurface)
        );
        // 0x23 and 0x24 are one type.
        assert_eq!(divine_type(DataId(0x2300_0001)), Some(DbType::StringTable));
        assert_eq!(divine_type(DataId(0x24FF_FFFF)), Some(DbType::StringTable));
        // 0x78-0x7F are one type.
        assert_eq!(divine_type(DataId(0x7800_0000)), Some(DbType::DbProperties));
        assert_eq!(divine_type(DataId(0x7FFF_FFFF)), Some(DbType::DbProperties));
    }

    /// The game table is searched first, which is the only reason `0x0E000002` is `CHAR_GEN` and
    /// not 0.
    #[test]
    fn game_ranges_win_over_engine_ranges() {
        // 0x30000000 is COMBAT_TABLE (game) and lies in no engine range.
        assert_eq!(divine_type(DataId(0x3000_0000)), Some(DbType::CombatTable));
        // WEENIE_DEF's range 0x00000001-0x0000FFFF is game-only.
        assert_eq!(divine_type(DataId(0x0000_1234)), Some(DbType::WeenieDef));
        assert_eq!(divine_type(DataId(0)), None);
    }

    /// Open question #112, resolved the way the client resolves it: registration order.
    #[test]
    fn the_font_local_string_state_overlap_resolves_by_registration_order() {
        assert_eq!(divine_type(DataId(0x4000_0000)), Some(DbType::Font));
        assert_eq!(divine_type(DataId(0x4000_0FFF)), Some(DbType::Font));
        assert_eq!(divine_type(DataId(0x4000_1000)), Some(DbType::FontLocal));
        assert_eq!(divine_type(DataId(0x40FF_FFFF)), Some(DbType::FontLocal));
        // The only id in this span that actually ships.
        assert_eq!(divine_type(DataId(0x4100_0000)), Some(DbType::StringState));
        assert_eq!(divine_type(DataId(0x41FF_FFFF)), Some(DbType::StringState));
    }

    /// `DivineType` returns 0 for every unused span between the mapped ranges.
    #[test]
    fn the_unused_spans_divine_to_nothing() {
        for id in [
            0x0B00_0000u32,
            0x0C00_0000,
            0x1A00_0000,
            0x1FFF_FFFF,
            0x2800_0000,
            0x2FFF_FFFF,
            0x3500_0000,
            0x37FF_FFFF,
            0x3A00_0000,
            0x3FFF_FFFF,
            0x4200_0000,
            0x77FF_FFFF,
            0x8000_0000,
            0xFFFE_FFFF,
            0xFFFF_0001,
        ] {
            assert_eq!(
                divine_type(DataId(id)),
                None,
                "{id:#010X} should divine to nothing"
            );
        }
    }

    /// Contract 9.7: exactly three categorized types.
    #[test]
    fn exactly_three_types_are_categorized() {
        let cats: Vec<DbType> = GAME_RANGES
            .iter()
            .chain(ENGINE_RANGES)
            .map(|(_, _, t)| *t)
            .filter(|t| t.is_categorized())
            .collect();
        assert_eq!(
            cats,
            vec![
                DbType::SurfaceTexture,
                DbType::RenderSurface,
                DbType::RenderTexture
            ]
        );
    }

    /// Section 5.3: the cell dat's ids are landblock-derived and its types come from the container.
    #[test]
    fn cell_ids_are_classified_by_position_not_by_divine_type() {
        assert_eq!(
            classify_cell_id(DataId(0xA9B4_FFFF)),
            Some(DbType::LandBlock)
        );
        assert_eq!(classify_cell_id(DataId(0xA9B4_FFFE)), Some(DbType::Lbi));
        assert_eq!(classify_cell_id(DataId(0xA9B4_0100)), Some(DbType::Cell));
        assert_eq!(classify_cell_id(DataId(0xA9B4_0001)), None);
        assert_eq!(classify_cell_id(DataId(0xA9B4_0040)), None);
        assert_eq!(classify_cell_id(ITERATION_LIST), None);
        // And the id that proves the point: 0x0000FFFF is a real landblock and would divine to
        // WEENIE_DEF if the cell dat used DivineType.
        assert_eq!(divine_type(DataId(0x0000_FFFF)), Some(DbType::WeenieDef));
        assert_eq!(
            classify_cell_id(DataId(0x0000_FFFF)),
            Some(DbType::LandBlock)
        );
    }

    #[test]
    fn routing_matches_the_catalogue() {
        assert_eq!(dat_for_type(DbType::GfxObj), DatKind::Portal);
        assert_eq!(dat_for_type(DbType::UiLayout), DatKind::Local);
        assert_eq!(dat_for_type(DbType::StringTable), DatKind::Local);
        assert_eq!(dat_for_type(DbType::FontLocal), DatKind::Local);
        assert_eq!(dat_for_type(DbType::StringState), DatKind::Local);
        assert_eq!(dat_for_type(DbType::Font), DatKind::Portal);
        assert_eq!(dat_for_type(DbType::LandBlock), DatKind::Cell);
        assert_eq!(dat_for_type(DbType::WeenieDef), DatKind::None);
    }
}
