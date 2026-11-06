//! Record types: the client's table of the 81 kinds of record, and how a record's id maps to its
//! type.
//!
//! A record stores no type. In the portal, local and high-resolution files the type is derived
//! from the id by a fixed chain of id ranges, tested in order, the first range holding the id
//! winning ([`type_of_id`]); ranges are half-open, so a range's registered top id (`0xXXFFFFFF`)
//! itself has no type. In the cell files a record is typed by its id's low word ([`cell_type`]).
//! So, as with the first game, never type an id by its top byte alone.
//!
//! Each type registers a file extension and a folder name, which name it here; the constants are
//! named for what the records hold.

use core::fmt;

/// A record type: the client's type number, `0x400000NN`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecordType(pub u32);

impl RecordType {
    pub const CELL_MESH: Self = Self(0x40000004);
    pub const SCENE: Self = Self(0x40000005);
    pub const ANIM_MAP: Self = Self(0x40000006);
    pub const SETUP: Self = Self(0x40000008);
    pub const ANIMATION: Self = Self(0x40000009);
    pub const MESH: Self = Self(0x4000000A);
    pub const REGION: Self = Self(0x4000000B);
    pub const SOUND_DESC: Self = Self(0x4000000C);
    pub const SCENE_DESC: Self = Self(0x4000000D);
    pub const TERRAIN_DESC: Self = Self(0x4000000E);
    pub const SURFACE_DESC: Self = Self(0x4000000F);
    pub const ENCOUNTER_DESC: Self = Self(0x40000010);
    pub const SKY_DESC: Self = Self(0x40000011);
    pub const PROPERTY_DESC: Self = Self(0x40000012);
    pub const WATER_DESC: Self = Self(0x40000013);
    pub const FOG_DESC: Self = Self(0x40000014);
    pub const BLOCK_MAP: Self = Self(0x40000015);
    pub const DAY_DESC: Self = Self(0x40000016);
    pub const WAVE: Self = Self(0x40000018);
    pub const QUALITIES: Self = Self(0x40000019);
    pub const QUALITY_FILTER: Self = Self(0x4000001D);
    pub const CONTENT_DB: Self = Self(0x4000001F);
    pub const VISUAL_DESC: Self = Self(0x40000021);
    pub const APPEARANCE: Self = Self(0x40000022);
    pub const UI_LAYOUT: Self = Self(0x40000025);
    pub const STRING_TABLE: Self = Self(0x40000026);
    pub const ENUM_MAP: Self = Self(0x40000028);
    pub const MUSIC: Self = Self(0x40000029);
    pub const ID_MAP: Self = Self(0x4000002A);
    pub const SOUND_ENVIRONMENT: Self = Self(0x4000002B);
    // D2-16808
    pub const SOUND_INFO: Self = Self(0x4000002C);
    // D2-16801
    pub const MUSIC_INFO: Self = Self(0x4000002D);
    pub const MATERIAL: Self = Self(0x4000002E);
    pub const SCRIPT_STATE: Self = Self(0x4000002F);
    pub const CHARACTER_TEMPLATE: Self = Self(0x40000030);
    pub const MATERIAL_MODIFIER: Self = Self(0x40000031);
    pub const MATERIAL_INSTANCE: Self = Self(0x40000032);
    pub const PHYSICS_MATERIAL: Self = Self(0x40000033);
    pub const MOTION_DESC: Self = Self(0x40000034);
    pub const PATH_MAP: Self = Self(0x40000035);
    pub const FILE_FORMAT: Self = Self(0x40000036);
    pub const TOOL_OBJECT: Self = Self(0x40000037);
    pub const TOOL_SHELL: Self = Self(0x40000038);
    pub const VALID_MODES: Self = Self(0x40000039);
    pub const PICTURE: Self = Self(0x4000003C);
    pub const TEXTURE: Self = Self(0x4000003D);
    pub const FONT: Self = Self(0x4000003E);
    pub const UI_SCENE: Self = Self(0x4000003F);
    pub const BEHAVIOR_TABLE: Self = Self(0x40000040);
    pub const PROPERTY_LIST: Self = Self(0x40000042);
    pub const GAME_TIME: Self = Self(0x40000043);
    pub const LANDSCAPE: Self = Self(0x40000044);
    pub const FILE_ID_TABLE: Self = Self(0x40000046);
    pub const PARTICLES: Self = Self(0x40000047);
    pub const ENTITY_GROUP: Self = Self(0x40000048);
    pub const ENTITY: Self = Self(0x40000049);
    pub const KEY_MAP: Self = Self(0x4000004A);
    pub const ACTION_MAP: Self = Self(0x4000004B);
    pub const FX_TABLE: Self = Self(0x4000004C);
    pub const WORD_FILTER: Self = Self(0x4000004D);
    pub const CAMERA_FX: Self = Self(0x4000004E);
    pub const SCRIPT_LIBRARY: Self = Self(0x4000004F);
    pub const LANDBLOCK: Self = Self(0x40000050);
    pub const LANDBLOCK_INFO: Self = Self(0x40000051);
    pub const CELL: Self = Self(0x40000052);
    pub const ITERATION_LIST: Self = Self(0x40000053);
    pub const PLACES: Self = Self(0x40000054);
    pub const STRING_STATE: Self = Self(0x40000055);
    pub const FX_SCRIPT: Self = Self(0x40000056);
    pub const CONVERSATION: Self = Self(0x40000057);
    pub const LIGHT_CACHE: Self = Self(0x40000058);
    pub const LIGHT_INFO: Self = Self(0x40000059);
    pub const DETAIL_MAP: Self = Self(0x4000005A);
    pub const OBSTACLE_DESC: Self = Self(0x4000005B);
    pub const ENCODED_WAVE: Self = Self(0x4000005C);
    pub const CATEGORIES: Self = Self(0x4000005D);
    pub const MAP_NOTES: Self = Self(0x4000005E);
    pub const PERFORMANCE: Self = Self(0x4000005F);
    pub const FONT_LOCAL: Self = Self(0x40000060);
    pub const PICTURE_LOCAL: Self = Self(0x40000061);
    pub const TEXTURE_LOCAL: Self = Self(0x40000062);
}

impl RecordType {
    /// This type's row of the table, if it is one of the 81.
    // D2-12737
    #[must_use]
    pub fn info(self) -> Option<&'static TypeInfo> {
        TYPES.iter().find(|t| t.ty == self)
    }

    /// Where the type's ids begin (0 for a type typed another way, or not one of the 81).
    // D2-12739
    #[must_use]
    pub fn base_id(self) -> u32 {
        self.info().map_or(0, |t| t.base)
    }

    /// Whether the type's records live in the portal file.
    // D2-12743
    #[must_use]
    pub fn is_portal_type(self) -> bool {
        self.info().is_some_and(|t| t.dat == DatKind::Portal)
    }

    /// Whether the type's records live in a cell file.
    // D2-12744
    #[must_use]
    pub fn is_cell_type(self) -> bool {
        self.info().is_some_and(|t| t.dat == DatKind::Cell)
    }

    /// Whether the type's records live in a language's local file.
    // D2-12745
    #[must_use]
    pub fn is_local_type(self) -> bool {
        self.info().is_some_and(|t| t.dat == DatKind::Local)
    }

    /// Whether the type's records carry a category word after their id.
    // D2-12746
    #[must_use]
    pub fn is_categorized(self) -> bool {
        self.info().is_some_and(|t| t.category_word)
    }

    /// How many of the type's records the client keeps ready once unused, and at most.
    // D2-12747
    #[must_use]
    pub fn cache_sizes(self) -> (u32, u32) {
        self.info().map_or((0, 0), |t| (t.cache_ideal, t.cache_max))
    }

    /// Whether the authoring tools keep each of the type's records as a file of its own (`false`
    /// for a type not in the table).
    // D2-12738
    #[must_use]
    pub fn has_file(self) -> bool {
        self.info().is_some_and(|t| t.has_file)
    }

    /// The type's pack version (0 for a type not in the table).
    // D2-12740
    #[must_use]
    pub fn pack_version(self) -> u32 {
        self.info().map_or(0, |t| t.pack_version)
    }

    /// Whether a new record of the type takes the place of the oldest unused one when more are
    /// kept unused than the type's ideal number: landblocks only.
    #[must_use]
    pub fn recycles_free_records(self) -> bool {
        self == Self::LANDBLOCK
    }

    /// A short name: the extension without its dot, or a name for the types registered without
    /// one.
    #[must_use]
    pub fn short_name(self) -> &'static str {
        self.info().map_or("unknown", |t| t.short)
    }
}

impl fmt::Debug for RecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RecordType({:08X} {})", self.0, self.short_name())
    }
}

impl fmt::Display for RecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08X} {}", self.0, self.short_name())
    }
}

/// The type table's own state: whether it is set up, and the folders the authoring tools keep the
/// game's and the engine's source files in. The client sets both folders as its database starts,
/// each the project's folder followed by `/`; a player's machine has no project, so both are `/`
/// ([`MasterMap::PLAYER_FOLDER`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MasterMap {
    set_up: bool,
    game_dir: String,
    engine_dir: String,
}

impl MasterMap {
    /// The game's and the engine's folder on a machine with no project settings.
    pub const PLAYER_FOLDER: &'static str = "/";

    /// Set the table up: `true` the first time; a table already set up is left as it is and
    /// answers `false`.
    // D2-12728
    pub fn init(&mut self) -> bool {
        if self.set_up {
            return false;
        }
        self.set_up = true;
        true
    }

    /// Let the table go (always `true`); the folders stay as they were set.
    // D2-12729
    pub fn cleanup(&mut self) -> bool {
        self.set_up = false;
        true
    }

    /// Whether the table is set up.
    #[must_use]
    pub fn is_set_up(&self) -> bool {
        self.set_up
    }

    /// The game's folder.
    #[must_use]
    pub fn game_folder(&self) -> &str {
        &self.game_dir
    }

    /// The engine's folder.
    #[must_use]
    pub fn engine_folder(&self) -> &str {
        &self.engine_dir
    }

    /// Set the game's folder.
    // D2-12734
    pub fn set_game_data_dir(&mut self, dir: &str) {
        dir.clone_into(&mut self.game_dir);
    }

    /// Set the engine's folder.
    // D2-12735
    pub fn set_engine_data_dir(&mut self, dir: &str) {
        dir.clone_into(&mut self.engine_dir);
    }

    /// The folder a type's source files are kept in under the game's folder: the game's folder
    /// followed by the type's folder name, or empty for a type with no folder name (or not in the
    /// table).
    // D2-12741
    #[must_use]
    pub fn game_data_dir(&self, ty: RecordType) -> String {
        Self::under(&self.game_dir, ty)
    }

    /// The same under the engine's folder.
    // D2-12742
    #[must_use]
    pub fn engine_data_dir(&self, ty: RecordType) -> String {
        Self::under(&self.engine_dir, ty)
    }

    fn under(root: &str, ty: RecordType) -> String {
        match ty.info().map_or("", |t| t.folder) {
            "" => String::new(),
            folder => format!("{root}{folder}"),
        }
    }
}

/// Which file a type's records live in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DatKind {
    Portal,
    Cell,
    Local,
    /// Registered but kept in no client file (the authoring tools' types).
    None,
}

/// One row of the client's type table.
// D2-12724
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeInfo {
    pub ty: RecordType,
    /// The registered id range, inclusive of `top` as registered (0, 0 for the cell types, which
    /// are typed by low word; a single id for the one-record types).
    pub base: u32,
    pub top: u32,
    /// The registered extension, with its dot (empty for the types registered without one).
    pub ext: &'static str,
    /// The name used in listings.
    pub short: &'static str,
    /// The registered folder name.
    pub folder: &'static str,
    pub dat: DatKind,
    /// Whether the client loads this type.
    pub client: bool,
    /// Whether a server loads this type.
    pub server: bool,
    /// Whether a record carries a category word after its id.
    pub category_word: bool,
    /// How many records of the type the client's cache keeps ready, and at most.
    pub cache_ideal: u32,
    pub cache_max: u32,
    /// The type's pack version (1 for every type).
    pub pack_version: u32,
    /// Whether the authoring tools keep each record as a file of its own (all but the landblock,
    /// the cell, the file-id table, the qualities and the iteration list).
    pub has_file: bool,
}

/// The client's 81 record types, in type-number order.
// D2-12727, D2-12011
pub static TYPES: &[TypeInfo] = &[
    TypeInfo {
        ty: RecordType::CELL_MESH,
        base: 0x01000000,
        top: 0x01FFFFFF,
        ext: ".cellmesh",
        short: "cellmesh",
        folder: "cellmesh",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SCENE,
        base: 0x02000000,
        top: 0x02FFFFFF,
        ext: ".scn",
        short: "scn",
        folder: "scn",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ANIM_MAP,
        base: 0x03000000,
        top: 0x03FFFFFF,
        ext: ".animmap",
        short: "animmap",
        folder: "animmap",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SETUP,
        base: 0x04000000,
        top: 0x04FFFFFF,
        ext: ".set",
        short: "set",
        folder: "setup",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ANIMATION,
        base: 0x05000000,
        top: 0x05FFFFFF,
        ext: ".anm",
        short: "anm",
        folder: "anim",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 128,
        cache_max: 256,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::MESH,
        base: 0x06000000,
        top: 0x06FFFFFF,
        ext: ".mesh",
        short: "mesh",
        folder: "mesh",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::REGION,
        base: 0x0F000000,
        top: 0x0FFFFFFF,
        ext: ".rgn",
        short: "rgn",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SOUND_DESC,
        base: 0x10000000,
        top: 0x10FFFFFF,
        ext: ".sod",
        short: "sod",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SCENE_DESC,
        base: 0x11000000,
        top: 0x11FFFFFF,
        ext: ".scd",
        short: "scd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::TERRAIN_DESC,
        base: 0x12000000,
        top: 0x12FFFFFF,
        ext: ".trd",
        short: "trd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SURFACE_DESC,
        base: 0x13000000,
        top: 0x13FFFFFF,
        ext: ".sfd",
        short: "sfd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ENCOUNTER_DESC,
        base: 0x14000000,
        top: 0x14FFFFFF,
        ext: ".end",
        short: "end",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SKY_DESC,
        base: 0x15000000,
        top: 0x15FFFFFF,
        ext: ".skd",
        short: "skd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::PROPERTY_DESC,
        base: 0x18000000,
        top: 0x18FFFFFF,
        ext: ".prd",
        short: "prd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::WATER_DESC,
        base: 0x16000000,
        top: 0x16FFFFFF,
        ext: ".wtd",
        short: "wtd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::FOG_DESC,
        base: 0x17000000,
        top: 0x17FFFFFF,
        ext: ".fgd",
        short: "fgd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::BLOCK_MAP,
        base: 0x19000000,
        top: 0x19FFFFFF,
        ext: ".bmd",
        short: "bmd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::DAY_DESC,
        base: 0x1A000000,
        top: 0x1AFFFFFF,
        ext: ".dyd",
        short: "dyd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::WAVE,
        base: 0x0C000000,
        top: 0x0CFFFFFF,
        ext: ".wav",
        short: "wav",
        folder: "sound",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 128,
        cache_max: 256,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::QUALITIES,
        base: 0x80000000,
        top: 0x81FFFFFF,
        ext: "",
        short: "qualities",
        folder: "",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 128,
        cache_max: 256,
        pack_version: 1,
        has_file: false,
    },
    TypeInfo {
        ty: RecordType::QUALITY_FILTER,
        base: 0x0A000000,
        top: 0x0AFFFFFF,
        ext: ".wqf",
        short: "wqf",
        folder: "weenie/filters",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::CONTENT_DB,
        base: 0x52000000,
        top: 0x527FFFFF,
        ext: ".cdb",
        short: "cdb",
        folder: "emp/contentor",
        dat: DatKind::Portal,
        client: false,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::VISUAL_DESC,
        base: 0x1F000000,
        top: 0x1FFFFFFF,
        ext: ".vd",
        short: "vd",
        folder: "vdesc",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::APPEARANCE,
        base: 0x20000000,
        top: 0x20FFFFFF,
        ext: ".apr",
        short: "apr",
        folder: "appearance",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::UI_LAYOUT,
        base: 0x22000000,
        top: 0x22FFFFFF,
        ext: ".uil",
        short: "uil",
        folder: "ui/layout",
        dat: DatKind::Local,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::STRING_TABLE,
        base: 0x25000000,
        top: 0x26FFFFFF,
        ext: ".stt",
        short: "stt",
        folder: "stringtable",
        dat: DatKind::Local,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ENUM_MAP,
        base: 0x23000000,
        top: 0x23FFFFFF,
        ext: ".emp",
        short: "emp",
        folder: "emp",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::MUSIC,
        base: 0x24000000,
        top: 0x24FFFFFF,
        ext: ".mid",
        short: "mid",
        folder: "sound",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ID_MAP,
        base: 0x28000000,
        top: 0x28FFFFFF,
        ext: ".imp",
        short: "imp",
        folder: "emp/idmap",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SOUND_ENVIRONMENT,
        base: 0x29000000,
        top: 0x29FFFFFF,
        ext: ".snv",
        short: "snv",
        folder: "sound/envinfo",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SOUND_INFO,
        base: 0x2A000000,
        top: 0x2AFFFFFF,
        ext: ".sif",
        short: "sif",
        folder: "sound",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 128,
        cache_max: 256,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::MUSIC_INFO,
        base: 0x2C000000,
        top: 0x2CFFFFFF,
        ext: ".mif",
        short: "mif",
        folder: "sound",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 128,
        cache_max: 256,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::MATERIAL,
        base: 0x2B000000,
        top: 0x2BFFFFFF,
        ext: ".mat",
        short: "mat",
        folder: "materials",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SCRIPT_STATE,
        base: 0x6F000000,
        top: 0x7FFFFFFF,
        ext: ".wc",
        short: "wc",
        folder: "weenie",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 128,
        cache_max: 256,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::CHARACTER_TEMPLATE,
        base: 0x2F000000,
        top: 0x2FFFFFFF,
        ext: ".ctp",
        short: "ctp",
        folder: "animsys",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::MATERIAL_MODIFIER,
        base: 0x30000000,
        top: 0x30FFFFFF,
        ext: ".mm",
        short: "mm",
        folder: "materials",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::MATERIAL_INSTANCE,
        base: 0x31000000,
        top: 0x31FFFFFF,
        ext: ".mi",
        short: "mi",
        folder: "materials",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::PHYSICS_MATERIAL,
        base: 0x37000000,
        top: 0x37FFFFFF,
        ext: ".pmat",
        short: "pmat",
        folder: "pmat",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::MOTION_DESC,
        base: 0x32000000,
        top: 0x32FFFFFF,
        ext: ".midesc",
        short: "midesc",
        folder: "animsys",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::PATH_MAP,
        base: 0x00000000,
        top: 0x00000000,
        ext: ".pathmap",
        short: "pathmap",
        folder: "pathmap",
        dat: DatKind::Cell,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::FILE_FORMAT,
        base: 0x27000000,
        top: 0x27FFFFFF,
        ext: ".fmt",
        short: "fmt",
        folder: "fmt",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::TOOL_OBJECT,
        base: 0x00000000,
        top: 0x00000000,
        ext: ".lbo",
        short: "lbo",
        folder: "lbo",
        dat: DatKind::None,
        client: false,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::TOOL_SHELL,
        base: 0x00000000,
        top: 0x00000000,
        ext: ".shell",
        short: "shell",
        folder: "shell",
        dat: DatKind::None,
        client: false,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::VALID_MODES,
        base: 0x54000000,
        top: 0x54FFFFFF,
        ext: ".validmodes",
        short: "validmodes",
        folder: "animsys",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::PICTURE,
        base: 0x41000000,
        top: 0x410FFFFF,
        ext: ".dds",
        short: "dds",
        folder: "surfaces",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: true,
        cache_ideal: 256,
        cache_max: 512,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::TEXTURE,
        base: 0x40000000,
        top: 0x400FFFFF,
        ext: ".texture",
        short: "texture",
        folder: "textures",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 128,
        cache_max: 256,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::FONT,
        base: 0x42000000,
        top: 0x42000FFF,
        ext: ".font",
        short: "font",
        folder: "fonts",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::UI_SCENE,
        base: 0x21000000,
        top: 0x21FFFFFF,
        ext: ".uiscene",
        short: "uiscene",
        folder: "ui/uiscene",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::BEHAVIOR_TABLE,
        base: 0x44000000,
        top: 0x44FFFFFF,
        ext: ".behaviortable",
        short: "behaviortable",
        folder: "behaviortable",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::PROPERTY_LIST,
        base: 0x34000000,
        top: 0x34FFFFFF,
        ext: ".mpr",
        short: "mpr",
        folder: "emp/property",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::GAME_TIME,
        base: 0x35000000,
        top: 0x35FFFFFF,
        ext: ".gmt",
        short: "gmt",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::LANDSCAPE,
        base: 0x36000000,
        top: 0x36FFFFFF,
        ext: ".lnd",
        short: "lnd",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::FILE_ID_TABLE,
        base: 0x0E000003,
        top: 0x0E000003,
        ext: "",
        short: "fileidtable",
        folder: "",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: false,
    },
    TypeInfo {
        ty: RecordType::PARTICLES,
        base: 0x39000000,
        top: 0x39FFFFFF,
        ext: ".psdesc",
        short: "psdesc",
        folder: "particles",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ENTITY_GROUP,
        base: 0x46000000,
        top: 0x46FFFFFF,
        ext: ".entitygroup",
        short: "entitygroup",
        folder: "entitygroup",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ENTITY,
        base: 0x47000000,
        top: 0x47FFFFFF,
        ext: ".entity",
        short: "entity",
        folder: "entity",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 64,
        cache_max: 128,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::KEY_MAP,
        base: 0x1D000000,
        top: 0x1DFFFFFF,
        ext: ".keymap",
        short: "keymap",
        folder: "keymap",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ACTION_MAP,
        base: 0x51000000,
        top: 0x51FFFFFF,
        ext: ".actionmap",
        short: "actionmap",
        folder: "actionmap",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::FX_TABLE,
        base: 0x1E000000,
        top: 0x1EFFFFFF,
        ext: ".fxtable",
        short: "fxtable",
        folder: "fxtable",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::WORD_FILTER,
        base: 0x0EBADA55,
        top: 0x0EBADA55,
        ext: ".taboo",
        short: "taboo",
        folder: "weenie",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::CAMERA_FX,
        base: 0x55000000,
        top: 0x55FFFFFF,
        ext: ".cfx",
        short: "cfx",
        folder: "anim",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::SCRIPT_LIBRARY,
        base: 0x56000000,
        top: 0x56FFFFFF,
        ext: ".wlib",
        short: "wlib",
        folder: "weenie/wsl",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::LANDBLOCK,
        base: 0x00000000,
        top: 0x00000000,
        ext: "",
        short: "landblock",
        folder: "",
        dat: DatKind::Cell,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 128,
        cache_max: 128,
        pack_version: 1,
        has_file: false,
    },
    TypeInfo {
        ty: RecordType::LANDBLOCK_INFO,
        base: 0x00000000,
        top: 0x00000000,
        ext: ".lbi",
        short: "lbi",
        folder: "lbi",
        dat: DatKind::Cell,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::CELL,
        base: 0x00000000,
        top: 0x00000000,
        ext: "",
        short: "cell",
        folder: "",
        dat: DatKind::Cell,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 128,
        pack_version: 1,
        has_file: false,
    },
    TypeInfo {
        ty: RecordType::ITERATION_LIST,
        base: 0xFFFF0000,
        top: 0xFFFFFFFF,
        ext: "",
        short: "iterations",
        folder: "",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: false,
    },
    TypeInfo {
        ty: RecordType::PLACES,
        base: 0x0E000004,
        top: 0x0E000004,
        ext: ".places",
        short: "places",
        folder: "weenie",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::STRING_STATE,
        base: 0x58000000,
        top: 0x58FFFFFF,
        ext: ".sti",
        short: "sti",
        folder: "stringtable",
        dat: DatKind::Local,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::FX_SCRIPT,
        base: 0x57000000,
        top: 0x57FFFFFF,
        ext: ".fxscript",
        short: "fxscript",
        folder: "fxscript",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::CONVERSATION,
        base: 0x60000000,
        top: 0x60FFFFFF,
        ext: ".conversation",
        short: "conversation",
        folder: "conversation",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::LIGHT_CACHE,
        base: 0x00000000,
        top: 0x00000000,
        ext: ".lightcache",
        short: "lightcache",
        folder: "shell",
        dat: DatKind::None,
        client: false,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::LIGHT_INFO,
        base: 0x00000000,
        top: 0x00000000,
        ext: ".lightinfo",
        short: "lightinfo",
        folder: "lbi",
        dat: DatKind::Cell,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 8,
        cache_max: 16,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::DETAIL_MAP,
        base: 0x59000000,
        top: 0x59000FFF,
        ext: ".detailmap",
        short: "detailmap",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::OBSTACLE_DESC,
        base: 0x38000000,
        top: 0x38FFFFFF,
        ext: ".obstacledesc",
        short: "obstacledesc",
        folder: "obsdesc",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::ENCODED_WAVE,
        base: 0x61000000,
        top: 0x61FFFFFF,
        ext: ".wv3",
        short: "wv3",
        folder: "sound",
        dat: DatKind::Local,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 1,
        cache_max: 32,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::CATEGORIES,
        base: 0x62000000,
        top: 0x62FFFFFF,
        ext: ".dbcategories",
        short: "dbcategories",
        folder: "emp",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::MAP_NOTES,
        base: 0x5900F000,
        top: 0x59FFFFFF,
        ext: ".mapnotes",
        short: "mapnotes",
        folder: "landscape",
        dat: DatKind::Portal,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::PERFORMANCE,
        base: 0x52800000,
        top: 0x52FFFFFF,
        ext: ".prf",
        short: "prf",
        folder: "emp/perfmon",
        dat: DatKind::Portal,
        client: true,
        server: true,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::FONT_LOCAL,
        base: 0x42001000,
        top: 0x42FFFFFF,
        ext: ".font_local",
        short: "font_local",
        folder: "fonts",
        dat: DatKind::Local,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 32,
        cache_max: 64,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::PICTURE_LOCAL,
        base: 0x41100000,
        top: 0x41FFFFFF,
        ext: ".dds_local",
        short: "dds_local",
        folder: "surfaces",
        dat: DatKind::Local,
        client: true,
        server: false,
        category_word: true,
        cache_ideal: 256,
        cache_max: 512,
        pack_version: 1,
        has_file: true,
    },
    TypeInfo {
        ty: RecordType::TEXTURE_LOCAL,
        base: 0x40100000,
        top: 0x40FFFFFF,
        ext: ".texture_local",
        short: "texture_local",
        folder: "textures",
        dat: DatKind::Local,
        client: true,
        server: false,
        category_word: false,
        cache_ideal: 128,
        cache_max: 256,
        pack_version: 1,
        has_file: true,
    },
];

/// The id-to-type chain, in the client's order: `(lo, hi, type)`, holding `lo <= id < hi`.
pub static ID_CHAIN: &[(u32, u32, RecordType)] = &[
    (0x46000000, 0x46FFFFFF, RecordType::ENTITY_GROUP),
    (0x47000000, 0x47FFFFFF, RecordType::ENTITY),
    (0x01000000, 0x01FFFFFF, RecordType::CELL_MESH),
    (0x02000000, 0x02FFFFFF, RecordType::SCENE),
    (0x03000000, 0x03FFFFFF, RecordType::ANIM_MAP),
    (0x04000000, 0x04FFFFFF, RecordType::SETUP),
    (0x05000000, 0x05FFFFFF, RecordType::ANIMATION),
    (0x06000000, 0x06FFFFFF, RecordType::MESH),
    (0x0C000000, 0x0CFFFFFF, RecordType::WAVE),
    (0x0E000003, 0x0E000004, RecordType::FILE_ID_TABLE),
    (0x0EBADA55, 0x0EBADA56, RecordType::WORD_FILTER),
    (0x0E000004, 0x0E000005, RecordType::PLACES),
    (0x0F000000, 0x0FFFFFFF, RecordType::REGION),
    (0x10000000, 0x10FFFFFF, RecordType::SOUND_DESC),
    (0x11000000, 0x11FFFFFF, RecordType::SCENE_DESC),
    (0x12000000, 0x12FFFFFF, RecordType::TERRAIN_DESC),
    (0x13000000, 0x13FFFFFF, RecordType::SURFACE_DESC),
    (0x14000000, 0x14FFFFFF, RecordType::ENCOUNTER_DESC),
    (0x15000000, 0x15FFFFFF, RecordType::SKY_DESC),
    (0x16000000, 0x16FFFFFF, RecordType::WATER_DESC),
    (0x17000000, 0x17FFFFFF, RecordType::FOG_DESC),
    (0x18000000, 0x18FFFFFF, RecordType::PROPERTY_DESC),
    (0x19000000, 0x19FFFFFF, RecordType::BLOCK_MAP),
    (0x1A000000, 0x1AFFFFFF, RecordType::DAY_DESC),
    (0x1D000000, 0x1DFFFFFF, RecordType::KEY_MAP),
    (0x1E000000, 0x1EFFFFFF, RecordType::FX_TABLE),
    (0x1F000000, 0x1FFFFFFF, RecordType::VISUAL_DESC),
    (0x20000000, 0x20FFFFFF, RecordType::APPEARANCE),
    (0x22000000, 0x22FFFFFF, RecordType::UI_LAYOUT),
    (0x21000000, 0x21FFFFFF, RecordType::UI_SCENE),
    (0x23000000, 0x23FFFFFF, RecordType::ENUM_MAP),
    (0x24000000, 0x24FFFFFF, RecordType::MUSIC),
    (0x25000000, 0x26FFFFFF, RecordType::STRING_TABLE),
    (0x27000000, 0x27FFFFFF, RecordType::FILE_FORMAT),
    (0x28000000, 0x28FFFFFF, RecordType::ID_MAP),
    (0x29000000, 0x29FFFFFF, RecordType::SOUND_ENVIRONMENT),
    (0x2A000000, 0x2AFFFFFF, RecordType::SOUND_INFO),
    (0x2B000000, 0x2BFFFFFF, RecordType::MATERIAL),
    (0x2C000000, 0x2CFFFFFF, RecordType::MUSIC_INFO),
    (0x2F000000, 0x2FFFFFFF, RecordType::CHARACTER_TEMPLATE),
    (0x30000000, 0x30FFFFFF, RecordType::MATERIAL_MODIFIER),
    (0x31000000, 0x31FFFFFF, RecordType::MATERIAL_INSTANCE),
    (0x32000000, 0x32FFFFFF, RecordType::MOTION_DESC),
    (0x34000000, 0x34FFFFFF, RecordType::PROPERTY_LIST),
    (0x35000000, 0x35FFFFFF, RecordType::GAME_TIME),
    (0x36000000, 0x36FFFFFF, RecordType::LANDSCAPE),
    (0x37000000, 0x37FFFFFF, RecordType::PHYSICS_MATERIAL),
    (0x38000000, 0x38FFFFFF, RecordType::OBSTACLE_DESC),
    (0x39000000, 0x39FFFFFF, RecordType::PARTICLES),
    (0x40000000, 0x400FFFFF, RecordType::TEXTURE),
    (0x40100000, 0x40FFFFFF, RecordType::TEXTURE_LOCAL),
    (0x41000000, 0x410FFFFF, RecordType::PICTURE),
    (0x41100000, 0x41FFFFFF, RecordType::PICTURE_LOCAL),
    (0x42000000, 0x42000FFF, RecordType::FONT),
    (0x42001000, 0x42FFFFFF, RecordType::FONT_LOCAL),
    (0x0A000000, 0x0AFFFFFF, RecordType::QUALITY_FILTER),
    (0x44000000, 0x44FFFFFF, RecordType::BEHAVIOR_TABLE),
    (0x51000000, 0x51FFFFFF, RecordType::ACTION_MAP),
    (0x52000000, 0x527FFFFF, RecordType::CONTENT_DB),
    (0x52800000, 0x52FFFFFF, RecordType::PERFORMANCE),
    (0x54000000, 0x54FFFFFF, RecordType::VALID_MODES),
    (0x55000000, 0x55FFFFFF, RecordType::CAMERA_FX),
    (0x56000000, 0x56FFFFFF, RecordType::SCRIPT_LIBRARY),
    (0x58000000, 0x58FFFFFF, RecordType::STRING_STATE),
    (0x57000000, 0x57FFFFFF, RecordType::FX_SCRIPT),
    (0x59000000, 0x59000FFF, RecordType::DETAIL_MAP),
    (0x5900F000, 0x59FFFFFF, RecordType::MAP_NOTES),
    (0x60000000, 0x60FFFFFF, RecordType::CONVERSATION),
    (0x61000000, 0x61FFFFFF, RecordType::ENCODED_WAVE),
    (0x62000000, 0x62FFFFFF, RecordType::CATEGORIES),
    (0x6F000000, 0x7FFFFFFF, RecordType::SCRIPT_STATE),
    (0x80000000, 0x81FFFFFF, RecordType::QUALITIES),
    (0xFFFF0000, 0xFFFFFFFF, RecordType::ITERATION_LIST),
];

/// The type of a portal, local or high-resolution record id, or `None` when no range holds it.
// D2-12723
#[must_use]
pub fn type_of_id(id: u32) -> Option<RecordType> {
    ID_CHAIN
        .iter()
        .find(|(lo, hi, _)| (*lo..*hi).contains(&id))
        .map(|c| c.2)
}

/// The type of a cell-file record, from its id's low word: `0xFFFF` the landblock, `0xFFFE` its
/// placed objects, `0xFFFD` its path map, `0xFFFC` its light, `0x0100` and up its cells; and the
/// iteration list.
#[must_use]
pub fn cell_type(id: u32) -> Option<RecordType> {
    if id == crate::container::ITERATION_LIST {
        return Some(RecordType::ITERATION_LIST);
    }
    match id & 0xFFFF {
        0xFFFF => Some(RecordType::LANDBLOCK),
        0xFFFE => Some(RecordType::LANDBLOCK_INFO),
        0xFFFD => Some(RecordType::PATH_MAP),
        0xFFFC => Some(RecordType::LIGHT_INFO),
        lo if lo >= 0x0100 => Some(RecordType::CELL),
        _ => None,
    }
}

/// The extensions a file name may end in, each with the type of its record, in the order they
/// are tried (letter case ignored).
const EXTENSION_CHAIN: &[(&str, RecordType)] = &[
    (".lbi", RecordType(0x40000051)),
    (".lightinfo", RecordType(0x40000059)),
    (".lbo", RecordType(0x40000037)),
    (".shell", RecordType(0x40000038)),
    (".lightcache", RecordType(0x40000058)),
    (".entitygroup", RecordType(0x40000048)),
    (".entity", RecordType(0x40000049)),
    (".pathmap", RecordType(0x40000035)),
    (".cellmesh", RecordType(0x40000004)),
    (".cellmeshinfo", RecordType(0x40000004)),
    (".scn", RecordType(0x40000005)),
    (".animmap", RecordType(0x40000006)),
    (".set", RecordType(0x40000008)),
    (".setinfo", RecordType(0x40000008)),
    (".anm", RecordType(0x40000009)),
    (".cube", RecordType(0x40000009)),
    (".amod", RecordType(0x40000009)),
    (".acube", RecordType(0x40000009)),
    (".anminfo", RecordType(0x40000009)),
    (".mesh", RecordType(0x4000000A)),
    (".obj", RecordType(0x4000000A)),
    (".meshinfo", RecordType(0x4000000A)),
    (".wav", RecordType(0x40000018)),
    (".taboo", RecordType(0x4000004D)),
    (".places", RecordType(0x40000054)),
    (".rgn", RecordType(0x4000000B)),
    (".sod", RecordType(0x4000000C)),
    (".scd", RecordType(0x4000000D)),
    (".trd", RecordType(0x4000000E)),
    (".sfd", RecordType(0x4000000F)),
    (".end", RecordType(0x40000010)),
    (".skd", RecordType(0x40000011)),
    (".wtd", RecordType(0x40000013)),
    (".fgd", RecordType(0x40000014)),
    (".prd", RecordType(0x40000012)),
    (".bmd", RecordType(0x40000015)),
    (".dyd", RecordType(0x40000016)),
    (".keymap", RecordType(0x4000004A)),
    (".fxtable", RecordType(0x4000004C)),
    (".vd", RecordType(0x40000021)),
    (".apr", RecordType(0x40000022)),
    (".uil", RecordType(0x40000025)),
    (".uiscene", RecordType(0x4000003F)),
    (".emp", RecordType(0x40000028)),
    (".mid", RecordType(0x40000029)),
    (".mus", RecordType(0x40000029)),
    (".sgt", RecordType(0x40000029)),
    (".dls", RecordType(0x40000029)),
    (".tpl", RecordType(0x40000029)),
    (".cdm", RecordType(0x40000029)),
    (".bnd", RecordType(0x40000029)),
    (".sty", RecordType(0x40000029)),
    (".aud", RecordType(0x40000029)),
    (".spt", RecordType(0x40000029)),
    (".stt", RecordType(0x40000026)),
    (".stt_ansi", RecordType(0x40000026)),
    (".stt_bin", RecordType(0x40000026)),
    (".fmt", RecordType(0x40000036)),
    (".imp", RecordType(0x4000002A)),
    (".snv", RecordType(0x4000002B)),
    (".sif", RecordType(0x4000002C)),
    (".mat", RecordType(0x4000002E)),
    (".mif", RecordType(0x4000002D)),
    (".ctp", RecordType(0x40000030)),
    (".mm", RecordType(0x40000031)),
    (".mi", RecordType(0x40000032)),
    (".midesc", RecordType(0x40000034)),
    (".mpr", RecordType(0x40000042)),
    (".gmt", RecordType(0x40000043)),
    (".lnd", RecordType(0x40000044)),
    (".pmat", RecordType(0x40000033)),
    (".obstacledesc", RecordType(0x4000005B)),
    (".psdesc", RecordType(0x40000047)),
    (".texture", RecordType(0x4000003D)),
    (".texture_local", RecordType(0x40000062)),
    (".dds", RecordType(0x4000003C)),
    (".tga", RecordType(0x4000003C)),
    (".iff", RecordType(0x4000003C)),
    (".jpg", RecordType(0x4000003C)),
    (".dds_local", RecordType(0x40000061)),
    (".tga_local", RecordType(0x40000061)),
    (".iff_local", RecordType(0x40000061)),
    (".jpg_local", RecordType(0x40000061)),
    (".font", RecordType(0x4000003E)),
    (".font_local", RecordType(0x40000060)),
    (".wqf", RecordType(0x4000001D)),
    (".behaviortable", RecordType(0x40000040)),
    (".actionmap", RecordType(0x4000004B)),
    (".cdb", RecordType(0x4000001F)),
    (".prf", RecordType(0x4000005F)),
    (".validmodes", RecordType(0x40000039)),
    (".cfx", RecordType(0x4000004E)),
    (".wlib", RecordType(0x4000004F)),
    (".sti", RecordType(0x40000055)),
    (".fxscript", RecordType(0x40000056)),
    (".detailmap", RecordType(0x4000005A)),
    (".mapnotes", RecordType(0x4000005E)),
    (".conversation", RecordType(0x40000057)),
    (".wv3", RecordType(0x4000005C)),
    (".dbcategories", RecordType(0x4000005D)),
    (".wc", RecordType(0x4000002F)),
];

/// The type of a record from a file name: the part from the name's first `.` on, compared without
/// regard to letter case with each extension the client knows; `None` for a name with no `.` or
/// an extension no type has.
// D2-12732, D2-12012, D2-12725
#[must_use]
pub fn type_of_file_name(name: &str) -> Option<RecordType> {
    let ext = &name[name.find('.')?..];
    EXTENSION_CHAIN
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(ext))
        .map(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the type table's internal consistency)
    use super::*;

    #[test]
    fn the_table_has_81_types_in_order_and_every_chain_target_is_in_it() {
        assert_eq!(TYPES.len(), 81);
        assert!(TYPES.windows(2).all(|w| w[0].ty < w[1].ty));
        for (_, _, ty) in ID_CHAIN {
            assert!(ty.info().is_some(), "{ty:?}");
        }
    }

    #[test]
    fn the_chain_agrees_with_every_registered_range() {
        for t in TYPES.iter().filter(|t| t.base != 0 && t.base != t.top) {
            assert_eq!(type_of_id(t.base), Some(t.ty), "{:?}", t.ty);
            assert_eq!(type_of_id(t.top - 1), Some(t.ty), "{:?}", t.ty);
        }
    }

    #[test]
    fn a_split_range_types_each_half_and_a_top_id_has_no_type() {
        assert_eq!(type_of_id(0x4100_0000), Some(RecordType::PICTURE));
        assert_eq!(type_of_id(0x4110_0000), Some(RecordType::PICTURE_LOCAL));
        assert_eq!(type_of_id(0x0EBA_DA55), Some(RecordType::WORD_FILTER));
        assert_eq!(type_of_id(0xFFFF_0001), Some(RecordType::ITERATION_LIST));
        assert_eq!(type_of_id(0x06FF_FFFF), None);
        assert_eq!(type_of_id(0x0700_0000), None);
    }

    #[test]
    fn a_file_name_is_typed_by_its_extension_from_the_first_dot() {
        assert_eq!(type_of_file_name("Human.EMP"), Some(RecordType::ENUM_MAP));
        assert_eq!(
            type_of_file_name("ui.stt_bin"),
            Some(RecordType::STRING_TABLE)
        );
        assert_eq!(type_of_file_name("rock.obj"), Some(RecordType::MESH));
        assert_eq!(type_of_file_name("a.b.emp"), None);
        assert_eq!(type_of_file_name("noext"), None);
        for t in TYPES.iter().filter(|t| !t.ext.is_empty()) {
            assert_eq!(
                type_of_file_name(&format!("x{}", t.ext)),
                Some(t.ty),
                "{}",
                t.ext
            );
        }
    }

    #[test]
    fn a_cell_record_is_typed_by_its_low_word() {
        assert_eq!(cell_type(0x5D3A_FFFF), Some(RecordType::LANDBLOCK));
        assert_eq!(cell_type(0x5D3A_0100), Some(RecordType::CELL));
        assert_eq!(cell_type(0x5D3A_0042), None);
    }
}
