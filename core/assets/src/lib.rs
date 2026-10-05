//! One pure decoder per data-file object type: geometry, setups, animations, palettes, textures and
//! tables.
//!
//! **Depends on** `dereth-dat` (the container and cursor) and `dereth-primitives`. **Used by** the
//! game rules (`dereth-rules`), the object model (`dereth-client-model`), audio (`dereth-audio`),
//! the shared world adapters (`dereth-world-data`), the client runtime and the SDK, the UI crates
//! (`dereth-ui`, `dereth-ui-screens`), world drawing (`dereth-world-render`), the client, and the
//! server (`empyrean-dat`, `empyrean-world`, `empyrean-testkit`).
//!
//! **Must never** interpret: palette expansion, texture and image decode, mesh construction,
//! motion-table expansion, collision and mixing belong to the rendering, physics, animation and
//! audio crates.
//!
//! Four rules apply to every decoder here:
//!
//! 1. **No `bytemuck`, no `#[repr(C)]` casts.** An `EnvCell`'s frame starts 2-aligned but not
//!    4-aligned whenever its surface count is odd. Every field goes through [`dereth_dat::Cursor`].
//! 2. **`read` and `write` are separate.** Field order *is* the format, and one bidirectional
//!    function that is wrong in one direction is the most expensive bug class there is.
//! 3. **Errors are `Result` and the first failure stops.** The original client's sticky, silent
//!    error latch is deliberately not reproduced, though it explains why some shipped data is
//!    slightly malformed.
//! 4. **`expect_end()` ends every decode**: the cursor lands exactly on the end of every record,
//!    across the whole data set, with nothing left over.
//!
//! **Specified in** `docs/formats/`: one page per record kind, pages 10 to 31, indexed by
//! `docs/formats/README.md`.

#![doc(html_no_source)]

pub mod audio;
pub mod common;
pub mod dbobj;
pub mod error;
/// The escape pass every string-table row goes through before it is shown. Here, beside
/// [`ui::StringTable`], so the UI and the headless client unescape a row the same way.
pub mod escape;
pub mod geometry;
pub mod hook;
pub mod material;
pub mod motion;
pub mod region;
pub mod subids;
pub mod tables;
pub mod texture_lookup;
pub mod ui;
pub mod verify;
pub mod world;

use dereth_dat::{ContainerEra, Cursor, DbType};
use dereth_primitives::DataId;

pub use audio::{SoundTable, Wave};
pub use common::{BspKind, BspNode, BspTree, Plane, Polygon, Sphere, SwVertex, VertexArray};
pub use dbobj::{check_id_echo, read_dbobj_header, DbObjHeader};
pub use error::AssetError;
pub use geometry::{Animation, Environment, GfxObj, Setup};
pub use hook::{AnimHook, HookData};
pub use material::{Palette, PaletteSet, RenderSurface, RenderTexture, Surface, SurfaceTexture};
pub use motion::{
    ClothingTable, GfxObjDegradeInfo, MotionTable, PhysicsScript, PhysicsScriptTable,
};
pub use region::Region;
pub use subids::{closure, SubDataIds};
pub use tables::{
    did_by_enum, Attribute2ndTable, BadData, CharGen, ChatPoseTable, CombatManeuverTable,
    ContractTable, DidMapper, DualDidMapper, EnumMapper, NameFilterTable, ObjectHierarchy,
    QualityFilter, QuestTable, SkillTable, SpellComponentTable, SpellTable, TabooTable, XpTable,
    MASTER_DID_MAPPER,
};
pub use ui::{
    ActionMap, Font, LanguageInfo, LanguageString, LayoutDesc, MasterInputMap, MasterProperty,
    PropertyAsset, StringTable,
};
pub use verify::{exhaustive_decode, exhaustive_decode_file, VerifyReport};
pub use world::{CellLandblock, EnvCell, LandblockInfo, ParticleEmitterInfo, Scene};

/// Every decoder is a pure function of bytes. No clock, no RNG, no I/O, no allocation policy.
pub trait Decode: Sized {
    /// The `DB_TYPE_*` this decoder reads.
    const TYPE: DbType;

    /// Read one object. Must consume the payload exactly; `decode_payload` enforces that.
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError>;

    /// The DataID the payload declared, for the echo check.
    ///
    /// `None` for the handful of types that carry no id prefix — surface records (`0x08`) are the one in
    /// retail data.
    fn declared_id(&self) -> Option<DataId>;

    /// Full payload to value, with `expect_end()` and the DataID echo check.
    fn decode_payload(id: DataId, bytes: &[u8]) -> Result<Self, AssetError> {
        let mut c = Cursor::new(bytes);
        let v = Self::decode(&mut c)?;
        c.expect_end()?;
        if let Some(found) = v.declared_id() {
            check_id_echo(id, found)?;
        }
        Ok(v)
    }

    /// Read one object in its layout from before Throne of Destiny (the `portal.dat` and
    /// `cell.dat` set). A type whose layout did not change there reads as [`Decode::decode`]; the
    /// types rewritten at Throne of Destiny override it. Must consume the payload exactly.
    fn decode_classic(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        Self::decode(c)
    }

    /// Full payload to value in the layout of the dat set `era` names, with `expect_end()` and the
    /// DataID echo check.
    fn decode_payload_in(era: ContainerEra, id: DataId, bytes: &[u8]) -> Result<Self, AssetError> {
        match era {
            ContainerEra::Modern => Self::decode_payload(id, bytes),
            ContainerEra::Classic => {
                let mut c = Cursor::new(bytes);
                let v = Self::decode_classic(&mut c)?;
                c.expect_end()?;
                if let Some(found) = v.declared_id() {
                    check_id_echo(id, found)?;
                }
                Ok(v)
            }
        }
    }

    /// The same, without the echo check, for callers testing a synthetic payload.
    fn decode_bytes(bytes: &[u8]) -> Result<Self, AssetError> {
        let mut c = Cursor::new(bytes);
        let v = Self::decode(&mut c)?;
        c.expect_end()?;
        Ok(v)
    }
}

/// The union of everything this crate can hand back, for the exhaustive-verify harness and for the
/// fixture generator. Consumers normally use the concrete types.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DecodedAsset {
    GfxObj(GfxObj),
    Setup(Setup),
    Animation(Animation),
    Palette(Palette),
    PaletteSet(PaletteSet),
    Surface(Surface),
    SurfaceTexture(SurfaceTexture),
    RenderSurface(RenderSurface),
    RenderTexture(RenderTexture),
    Environment(Environment),
    MotionTable(MotionTable),
    Wave(Wave),
    SoundTable(SoundTable),
    DegradeInfo(GfxObjDegradeInfo),
    ClothingTable(ClothingTable),
    Scene(Scene),
    Region(Region),
    ParticleEmitterInfo(ParticleEmitterInfo),
    PhysicsScript(PhysicsScript),
    PhysicsScriptTable(PhysicsScriptTable),
    Landblock(CellLandblock),
    LandblockInfo(LandblockInfo),
    EnvCell(EnvCell),
    SpellTable(Box<SpellTable>),
    SpellComponentTable(SpellComponentTable),
    SkillTable(SkillTable),
    XpTable(XpTable),
    Attribute2ndTable(Attribute2ndTable),
    CharGen(Box<CharGen>),
    ChatPoseTable(ChatPoseTable),
    CombatManeuverTable(CombatManeuverTable),
    ContractTable(Box<ContractTable>),
    ObjectHierarchy(ObjectHierarchy),
    TabooTable(TabooTable),
    NameFilterTable(NameFilterTable),
    BadData(BadData),
    QualityFilter(QualityFilter),
    QuestTable(QuestTable),
    EnumMapper(EnumMapper),
    DidMapper(DidMapper),
    DualDidMapper(DualDidMapper),
    StringTable(Box<StringTable>),
    LanguageString(LanguageString),
    LanguageInfo(Box<LanguageInfo>),
    Font(Font),
    ActionMap(ActionMap),
    MasterInputMap(MasterInputMap),
    MasterProperty(Box<MasterProperty>),
}

macro_rules! dispatch {
    ($era:expr, $kind:expr, $id:expr, $bytes:expr,
     plain { $( $t:ident => $v:ident($ty:ty) ),* $(,)? }
     boxed { $( $bt:ident => $bv:ident($bty:ty) ),* $(,)? }) => {
        match $kind {
            $( DbType::$t => Ok(DecodedAsset::$v(<$ty as Decode>::decode_payload_in($era, $id, $bytes)?)), )*
            $( DbType::$bt =>
                Ok(DecodedAsset::$bv(Box::new(<$bty as Decode>::decode_payload_in($era, $id, $bytes)?))), )*
            other => Err(AssetError::NoDecoder(other)),
        }
    };
}

/// Decode one payload, given its type.
///
/// Two types are absent by construction, not by omission: `UI_LAYOUT` (`0x21`) and `DBPROPERTIES`
/// (`0x78`) need the `MasterProperty` type map to know what shape each property value has, exactly
/// as the client does. Use [`ui::LayoutDesc::decode_payload`] and
/// [`ui::PropertyAsset::decode_payload`], which take that map.
pub fn decode_any(kind: DbType, id: DataId, bytes: &[u8]) -> Result<DecodedAsset, AssetError> {
    decode_any_in(ContainerEra::Modern, kind, id, bytes)
}

/// [`decode_any`] in the record layouts of the dat set `era` names: before Throne of Destiny the
/// rewritten types read their older layout ([`Decode::decode_classic`]) into the same values.
pub fn decode_any_in(
    era: ContainerEra,
    kind: DbType,
    id: DataId,
    bytes: &[u8],
) -> Result<DecodedAsset, AssetError> {
    dispatch!(era, kind, id, bytes,
        plain {
            GfxObj => GfxObj(GfxObj),
            Setup => Setup(Setup),
            Anim => Animation(Animation),
            Palette => Palette(Palette),
            PalSet => PaletteSet(PaletteSet),
            Surface => Surface(Surface),
            SurfaceTexture => SurfaceTexture(SurfaceTexture),
            RenderSurface => RenderSurface(RenderSurface),
            RenderTexture => RenderTexture(RenderTexture),
            Environment => Environment(Environment),
            MTable => MotionTable(MotionTable),
            Wave => Wave(Wave),
            STable => SoundTable(SoundTable),
            DegradeInfo => DegradeInfo(GfxObjDegradeInfo),
            Clothing => ClothingTable(ClothingTable),
            Scene => Scene(Scene),
            Region => Region(Region),
            ParticleEmitter => ParticleEmitterInfo(ParticleEmitterInfo),
            PhysicsScript => PhysicsScript(PhysicsScript),
            PhysicsScriptTable => PhysicsScriptTable(PhysicsScriptTable),
            LandBlock => Landblock(CellLandblock),
            Lbi => LandblockInfo(LandblockInfo),
            Cell => EnvCell(EnvCell),
            SpellComponentTable => SpellComponentTable(SpellComponentTable),
            SkillTable => SkillTable(SkillTable),
            XpTable => XpTable(XpTable),
            Attribute2ndTable => Attribute2ndTable(Attribute2ndTable),
            ChatPoseTable => ChatPoseTable(ChatPoseTable),
            CombatTable => CombatManeuverTable(CombatManeuverTable),
            ObjectHierarchy => ObjectHierarchy(ObjectHierarchy),
            TabooTable => TabooTable(TabooTable),
            NameFilterTable => NameFilterTable(NameFilterTable),
            BadData => BadData(BadData),
            QualityFilter => QualityFilter(QualityFilter),
            QuestDefDb => QuestTable(QuestTable),
            EnumMapper => EnumMapper(EnumMapper),
            DidMapper => DidMapper(DidMapper),
            DualDidMapper => DualDidMapper(DualDidMapper),
            StringType => LanguageString(LanguageString),
            Font => Font(Font),
            ActionMap => ActionMap(ActionMap),
            Keymap => MasterInputMap(MasterInputMap),
        }
        boxed {
            SpellTable => SpellTable(SpellTable),
            CharGen => CharGen(CharGen),
            ContractTable => ContractTable(ContractTable),
            StringTable => StringTable(StringTable),
            StringState => LanguageInfo(LanguageInfo),
            MasterProperty => MasterProperty(MasterProperty),
        }
    )
}

/// The types [`decode_any`] can handle today.
#[must_use]
pub fn decodable_types() -> &'static [DbType] {
    &[
        DbType::GfxObj,
        DbType::Setup,
        DbType::Anim,
        DbType::Palette,
        DbType::PalSet,
        DbType::Surface,
        DbType::SurfaceTexture,
        DbType::RenderSurface,
        DbType::RenderTexture,
        DbType::Environment,
        DbType::MTable,
        DbType::Wave,
        DbType::STable,
        DbType::DegradeInfo,
        DbType::Clothing,
        DbType::Scene,
        DbType::Region,
        DbType::ParticleEmitter,
        DbType::PhysicsScript,
        DbType::PhysicsScriptTable,
        DbType::LandBlock,
        DbType::Lbi,
        DbType::Cell,
        DbType::SpellTable,
        DbType::SpellComponentTable,
        DbType::SkillTable,
        DbType::XpTable,
        DbType::Attribute2ndTable,
        DbType::CharGen,
        DbType::ChatPoseTable,
        DbType::CombatTable,
        DbType::ContractTable,
        DbType::ObjectHierarchy,
        DbType::TabooTable,
        DbType::NameFilterTable,
        DbType::BadData,
        DbType::QualityFilter,
        DbType::QuestDefDb,
        DbType::EnumMapper,
        DbType::DidMapper,
        DbType::DualDidMapper,
        DbType::StringTable,
        DbType::StringType,
        DbType::StringState,
        DbType::Font,
        DbType::ActionMap,
        DbType::Keymap,
        DbType::MasterProperty,
    ]
}
