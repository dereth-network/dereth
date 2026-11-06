//! Any one record, by its type: [`Record::decode`] picks the decoder from the record's type.

use std::any::{Any, TypeId};
use std::sync::Arc;

use auberean_dat::reader::Result;
use auberean_dat::RecordType;

use crate::{
    animation, entity, fx, landblock, material, mesh, picture, property, region, script, setup,
    sound, strings, tables, ui, visual, Context,
};

macro_rules! records {
    ($($($ty:ident)|+ => $variant:ident($path:ty),)*) => {
        /// A decoded record of any type.
        // A record is decoded one at a time and handed on, never stored in bulk, so the size of
        // its largest variant costs nothing worth boxing for.
        #[allow(clippy::large_enum_variant)]
        #[derive(Clone, Debug, PartialEq)]
        pub enum Record {
            $($variant($path),)*
        }

        impl Record {
            /// Decode a record of a given type, every byte consumed; `None` when the type has no
            /// decoder (the authoring tools' types, which no client file holds).
            pub fn decode(ty: RecordType, id: u32, bytes: &[u8], ctx: &Context) -> Option<Result<Self>> {
                $(
                    if $(ty == RecordType::$ty)||+ {
                        return Some(crate::decode::<$path>(id, bytes, ctx).map(Self::$variant));
                    }
                )*
                None
            }

            /// Whether a type has a decoder.
            #[must_use]
            pub fn has_decoder(ty: RecordType) -> bool {
                $($(ty == RecordType::$ty)||+ ||)* false
            }

            /// The Rust type a record type decodes as (the caches hold a record by it).
            pub(crate) fn rust_type(ty: RecordType) -> Option<TypeId> {
                $(
                    if $(ty == RecordType::$ty)||+ {
                        return Some(TypeId::of::<$path>());
                    }
                )*
                None
            }

            /// The record as the caches hold it: its own type, shared.
            pub(crate) fn into_held(self) -> Arc<dyn Any + Send + Sync> {
                match self {
                    $(Self::$variant(v) => Arc::new(v),)*
                }
            }
        }
    };
}

// One cache per type, each made with its type's allocator: a type here is what the caches hold.
// D2-05180
records! {
    CELL_MESH => CellMesh(mesh::CellMesh),
    SCENE => Scene(visual::Scene),
    ANIM_MAP => AnimMap(animation::AnimMap),
    SETUP => Setup(setup::Setup),
    ANIMATION => Animation(animation::Animation),
    MESH => Mesh(mesh::Mesh),
    REGION => Region(region::Region),
    SOUND_DESC => SoundDesc(region::SoundDesc),
    SCENE_DESC => SceneDesc(region::SceneDesc),
    TERRAIN_DESC => TerrainDesc(region::TerrainDesc),
    SURFACE_DESC => SurfaceDesc(region::SurfaceDesc),
    ENCOUNTER_DESC => EncounterDesc(region::EncounterDesc),
    SKY_DESC => SkyDesc(region::SkyDesc),
    PROPERTY_DESC => PropertyDesc(region::PropertyDesc),
    WATER_DESC => WaterDesc(region::WaterDesc),
    FOG_DESC => FogDesc(region::FogDesc),
    BLOCK_MAP => BlockMap(region::BlockMap),
    DAY_DESC => DayDesc(region::DayDesc),
    WAVE => Wave(sound::Wave),
    QUALITIES => Qualities(script::Qualities),
    QUALITY_FILTER => QualityFilter(script::QualityFilter),
    CONTENT_DB => ContentDb(tables::ContentDb),
    VISUAL_DESC => VisualDesc(visual::VisualDesc),
    APPEARANCE => Appearance(visual::Appearance),
    UI_LAYOUT => UiLayout(ui::UiLayout),
    STRING_TABLE => StringTable(strings::StringTable),
    ENUM_MAP => EnumMap(strings::EnumMap),
    MUSIC => Music(sound::Music),
    ID_MAP => IdMap(tables::IdMap),
    SOUND_ENVIRONMENT => SoundEnvironment(sound::SoundEnvironment),
    SOUND_INFO => SoundInfo(sound::SoundInfo),
    MUSIC_INFO => MusicInfo(sound::MusicInfo),
    MATERIAL => Material(material::Material),
    SCRIPT_STATE => ScriptState(script::ScriptState),
    CHARACTER_TEMPLATE => CharacterTemplate(animation::CharacterTemplate),
    MATERIAL_MODIFIER => MaterialModifier(material::MaterialModifier),
    MATERIAL_INSTANCE => MaterialInstance(material::MaterialInstance),
    PHYSICS_MATERIAL => PhysicsMaterial(property::PhysicsMaterial),
    MOTION_DESC => MotionDesc(animation::MotionDesc),
    PATH_MAP => PathMap(tables::PathMap),
    FILE_FORMAT => FileFormat(tables::FileFormat),
    VALID_MODES => AllowedModes(animation::AllowedModes),
    PICTURE | PICTURE_LOCAL => Picture(picture::Picture),
    TEXTURE | TEXTURE_LOCAL => Texture(picture::Texture),
    FONT | FONT_LOCAL => Font(ui::Font),
    UI_SCENE => UiScene(ui::UiScene),
    BEHAVIOR_TABLE => BehaviorTable(animation::BehaviorTable),
    PROPERTY_LIST => PropertyList(property::PropertyList),
    GAME_TIME => GameTime(region::GameTime),
    LANDSCAPE => Landscape(region::Landscape),
    FILE_ID_TABLE => FileIdTable(tables::FileIdTable),
    PARTICLES => Particles(fx::Particles),
    ENTITY_GROUP => EntityGroup(entity::EntityGroup),
    ENTITY => Entity(entity::Entity),
    KEY_MAP => KeyMap(ui::KeyMap),
    ACTION_MAP => ActionBindings(ui::ActionBindings),
    FX_TABLE => FxTable(fx::FxTable),
    WORD_FILTER => WordFilter(script::WordFilter),
    CAMERA_FX => CameraFx(fx::CameraFx),
    SCRIPT_LIBRARY => ScriptLibrary(script::ScriptLibrary),
    LANDBLOCK => Landblock(landblock::Landblock),
    LANDBLOCK_INFO => LandblockInfo(landblock::LandblockInfo),
    CELL => Cell(landblock::Cell),
    ITERATION_LIST => IterationList(tables::IterationList),
    PLACES => Places(tables::Places),
    STRING_STATE => StringState(strings::StringState),
    FX_SCRIPT => FxScript(fx::FxScript),
    CONVERSATION => Conversation(script::Conversation),
    LIGHT_INFO => LightInfo(landblock::LightInfo),
    DETAIL_MAP => DetailMap(region::DetailMap),
    OBSTACLE_DESC => Obstacles(tables::Obstacles),
    ENCODED_WAVE => LocalSound(sound::LocalSound),
    CATEGORIES => Categories(tables::Categories),
    MAP_NOTES => MapNotes(region::MapNotes),
    PERFORMANCE => Performance(tables::Performance),
}
