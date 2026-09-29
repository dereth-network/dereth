//! The `world.pack` container (our own format, carried from the v1 server) and its table ids.
//!
//! Which records live in which table, and under which key, is [`TableId`]'s documentation; the
//! record layouts are the World-DB models in [`crate::models::world`], encoded by
//! [`crate::records`].

pub mod cursor;
pub mod format;
pub mod index;
pub mod reader;
pub mod writer;

pub use cursor::{decode, encode, Codec, Cursor, PackWrite};
pub use format::{IndexEntry, PackHeader, TableDirEntry};
pub use reader::{hex, Pack};
pub use writer::{write_atomically, BuildStats, PackWriter};

/// A pack table.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableId(pub u16);

impl std::fmt::Debug for TableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({:#x})", self.name(), self.0)
    }
}

macro_rules! tables {
    ($($(#[$doc:meta])* $name:ident = $id:literal, $text:literal;)*) => {
        impl TableId {
            $( $(#[$doc])* pub const $name: TableId = TableId($id); )*

            /// Every table this build writes, ascending.
            pub const ALL: &'static [TableId] = &[$(TableId::$name),*];

            /// The table's name (16 bytes at most, stored in the directory).
            #[must_use]
            pub fn name(self) -> &'static str {
                match self.0 {
                    $( $id => $text, )*
                    _ => "unknown",
                }
            }
        }
    };
}

tables! {
    /// `weenie` with all 23 `weenie_properties_*` child tables embedded (emote actions inside their
    /// emotes); key: class id. Children are in primary-key (dump) order.
    WEENIE = 1, "weenie";
    /// Derived: one small [`crate::records::WeenieIndex`] per weenie (class name, type, `Name`,
    /// scroll spell) for the queries that scan every weenie; key: class id.
    WEENIE_INDEX = 2, "weenie_index";
    /// `landblock_instance` with `landblock_instance_link` embedded, grouped by landblock; key:
    /// landblock (`obj_Cell_Id >> 16`). A `Vec` in guid order.
    LANDBLOCK_INSTANCE = 3, "landblock_inst";
    /// `encounter` grouped by landblock; key: landblock as `u32`. A `Vec` in id order.
    ENCOUNTER = 4, "encounter";
    /// `cook_book` grouped by `(source_W_C_I_D << 32) | target_W_C_I_D`. A `Vec` in id order.
    COOK_BOOK = 5, "cook_book";
    /// `recipe` with its mods (and their six stat tables) and six requirement tables; key: id.
    RECIPE = 6, "recipe";
    /// `event`; key: id.
    EVENT = 7, "event";
    /// `house_portal`; key: id.
    HOUSE_PORTAL = 8, "house_portal";
    /// `points_of_interest`; key: id.
    POINTS_OF_INTEREST = 9, "poi";
    /// `quest`; key: id.
    QUEST = 10, "quest";
    /// `spell`; key: id.
    SPELL = 11, "spell";
    /// `treasure_death`; key: id.
    TREASURE_DEATH = 12, "treasure_death";
    /// `treasure_gem_count`; key: id.
    TREASURE_GEM_COUNT = 13, "treasure_gem";
    /// `treasure_material_base`; key: id.
    TREASURE_MATERIAL_BASE = 14, "tmat_base";
    /// `treasure_material_color`; key: id.
    TREASURE_MATERIAL_COLOR = 15, "tmat_color";
    /// `treasure_material_groups`; key: id.
    TREASURE_MATERIAL_GROUPS = 16, "tmat_groups";
    /// `treasure_wielded`; key: id.
    TREASURE_WIELDED = 17, "treasure_wielded";
    /// `version`; key: id.
    VERSION = 18, "version";
}
