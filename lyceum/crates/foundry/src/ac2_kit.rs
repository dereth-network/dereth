//! Asheron's Call 2's dungeon kits in a world: every piece of a kit (its cell meshes, at their
//! full size), each one environment, and each of the kit's looks a list of surfaces for it, made
//! from the player's own Asheron's Call 2 install ([`dereth_ac2_convert`]).
//!
//! A pack asks for a kit with an `[[ac2-kit]]` row in its records; a build then reads the install
//! (`--ac2-dats`), and every piece and look of the kit becomes records of the world, numbered in its
//! lockfile by what each is, so the same install always gives the same ids. A piece is placed as any
//! of the world's pieces is (`[[cell]]`), with its environment and one of its looks' surfaces.
//!
//! **The kits.** `basic` (stone halls and rooms) and `fancy` (a palace's), both on a 10 m grid:
//! hall tiles 10 m a side and 10 m a floor, open on every side they share with another, and halls
//! 20 m a side with 10 by 9 m doorways. `tomb` (rough tunnels and vaulted rooms), on a 15 m grid:
//! halls a square, 15 by 8 m doorways, rooms of 8 m floor tiles under 4 m roofs (with 8 m middles
//! between for a taller room), entered from a hall through a room entrance. `mine` (timbered
//! tunnels, 20 m squares) and `castle` (halls, great rooms and towers, 20 m squares). `cave`
//! (tunnels of 10, 20 and 30 m, wide and narrow, bending by eighths of a turn, forking and
//! climbing): it stands on no squares, each piece set at a doorway, and its space is the cells its
//! mesh walls in.
//!
//! **Looks.** An AC2 cell can name an appearance that swaps its mesh's materials. Each kit's looks
//! are its own materials (`own`) and the appearances AC2's dungeons draw it in: `plain`, `haunted`
//! and `emplite` for `basic`, `broken` for `fancy`, `human` for `castle`; for `cave`, the stones
//! AC2's caves are drawn in.
//!
//! **On a converted land.** A land converted from AC2 (New Dereth's) already holds every kit
//! piece: each AC2 cell mesh its rooms and dungeons are drawn with is one of its environments, in
//! the mesh's own materials, its cell tree cut at its doorways. The land says which in its overlay
//! folder ([`LAND_TABLE`]). A world over such a land takes its kits' pieces from it
//! ([`from_land`]): no environment or surface is made for them, a new dungeon's cells name the
//! land's records, and each piece has the one look the land drew it in (`own`), which nothing
//! changes. No install is needed for them; the kits' lights still come from one when it is given.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use dereth_ac2_convert::records::{NewIds, Pictures, Records};
use dereth_ac2_convert::source::Ac2;
use dereth_primitives::DataId;

/// The player's Asheron's Call 2 install, open, to make the kits from.
pub type Ac2Install = Ac2;

/// The player's Asheron's Call 2 install in folder `dir`, open, to make the kits from.
///
/// # Errors
/// A file of the install that will not open.
pub fn open(dir: &std::path::Path) -> Result<Ac2Install, String> {
    Ac2::open(dir)
}

/// How a kit's pieces' space is judged ([`Kit::space`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// The box its cell tree is: right for a piece that fills its box.
    Box,
    /// The cells of its box its mesh walls in: for pieces that bend and fork, and fill little of
    /// it.
    Voxels,
}

/// One of AC2's dungeon kits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kit {
    Basic,
    Fancy,
    Tomb,
    Mine,
    Castle,
    Cave,
}

impl Kit {
    /// Every kit, in order.
    pub const ALL: [Self; 6] = [
        Self::Basic,
        Self::Fancy,
        Self::Tomb,
        Self::Mine,
        Self::Castle,
        Self::Cave,
    ];

    /// Its name, as a pack writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Fancy => "fancy",
            Self::Tomb => "tomb",
            Self::Mine => "mine",
            Self::Castle => "castle",
            Self::Cave => "cave",
        }
    }

    /// The grid its pieces stand on: basic's and fancy's 10 m squares, the tomb's 15 m, the mine's
    /// and the castle's 20 m; room tiles a whole square tall (basic, fancy) or of any height (the
    /// tomb's 4 m roofs). The castle's halls are a square across, so a piece of it two squares
    /// across is a room.
    #[must_use]
    pub const fn grid(self) -> crate::kit::Grid {
        match self {
            Self::Basic | Self::Fancy => crate::kit::Grid::WORLD,
            Self::Tomb => crate::kit::Grid {
                pitch: 15.0,
                tile_height: 0.0,
                two_square_halls: true,
                squares: true,
            },
            Self::Mine => crate::kit::Grid {
                pitch: 20.0,
                tile_height: crate::kit::TILE_HEIGHT,
                two_square_halls: true,
                squares: true,
            },
            Self::Castle => crate::kit::Grid {
                pitch: 20.0,
                tile_height: crate::kit::TILE_HEIGHT,
                two_square_halls: false,
                squares: true,
            },
            Self::Cave => crate::kit::Grid {
                pitch: 20.0,
                tile_height: crate::kit::TILE_HEIGHT,
                two_square_halls: false,
                squares: false,
            },
        }
    }

    /// How far apart its dungeons' floors stand, in meters: a room's floor tile and what stands
    /// over it (basic's and fancy's 10 m tiles; the tomb's 8 m floor and 4 m roof); the mine's
    /// 10 m tunnels; the castle's 10 m halls and tower floors; a cave's 20 m tunnels (its climbs
    /// stand where they rise to, as floors of their own).
    #[must_use]
    pub const fn rise(self) -> f32 {
        match self {
            Self::Basic | Self::Fancy | Self::Mine | Self::Castle => 10.0,
            Self::Tomb => 12.0,
            Self::Cave => 20.0,
        }
    }

    /// The piece a hall joins one of its rooms through, when it has one: the tomb's halls are
    /// narrower than its rooms' sides, and no hall in AC2's own tombs meets a room tile directly
    /// (none of the 1,801 links that touch a tomb piece, in the 16 blocks that use one): a room
    /// entrance stands between, its first doorway to the room in every one of the 72.
    #[must_use]
    pub const fn room_entrance(self) -> Option<&'static str> {
        match self {
            Self::Basic | Self::Fancy | Self::Mine | Self::Castle | Self::Cave => None,
            Self::Tomb => Some("room floor entrance"),
        }
    }

    /// Whether it has room tiles for a room to be laid of (the mine and the cave have none, only
    /// tunnels; the castle's rooms are whole pieces).
    #[must_use]
    pub const fn has_rooms(self) -> bool {
        !matches!(self, Self::Mine | Self::Castle | Self::Cave)
    }

    /// How its pieces' space is judged when one is fitted: by the box its cell tree is, or by the
    /// cells of that box its mesh walls in ([`crate::room::Shape::of_voxels`]).
    #[must_use]
    pub const fn space(self) -> Space {
        match self {
            Self::Basic | Self::Fancy | Self::Tomb | Self::Mine | Self::Castle => Space::Box,
            // Its tunnels bend and fork, filling little of their boxes.
            Self::Cave => Space::Voxels,
        }
    }

    /// How tall a room's floor tile is, in meters: where the tile over it stands.
    #[must_use]
    pub const fn layer(self) -> f32 {
        match self {
            Self::Basic | Self::Fancy | Self::Mine | Self::Castle => 10.0,
            Self::Tomb => 8.0,
            Self::Cave => 20.0,
        }
    }

    /// The kit called `name`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The AC2 dungeons built of it, by block: their lights are the kit's lights: every dungeon
    /// it is the main kit of (`basic` 109, `fancy` 27, `tomb` 11, `mine` 1, `castle` 9).
    #[must_use]
    pub const fn dungeons(self) -> &'static [(i32, i32)] {
        match self {
            Self::Basic => &[
                (0x00, 0xDE),
                (0x00, 0xEC),
                (0x01, 0x01),
                (0x01, 0xAC),
                (0x01, 0xAF),
                (0x01, 0xBB),
                (0x01, 0xC9),
                (0x01, 0xCA),
                (0x01, 0xCC),
                (0x01, 0xD1),
                (0x01, 0xD3),
                (0x01, 0xD6),
                (0x01, 0xDA),
                (0x01, 0xDC),
                (0x01, 0xDF),
                (0x01, 0xEE),
                (0x01, 0xEF),
                (0x01, 0xF1),
                (0x01, 0xF2),
                (0x01, 0xF4),
                (0x01, 0xF5),
                (0x01, 0xF7),
                (0x01, 0xF8),
                (0x01, 0xFA),
                (0x01, 0xFB),
                (0x01, 0xFD),
                (0x02, 0xA0),
                (0x02, 0xBB),
                (0x03, 0x7D),
                (0x03, 0x80),
                (0x03, 0x84),
                (0x03, 0xBB),
                (0x03, 0xC4),
                (0x03, 0xC5),
                (0x03, 0xC6),
                (0x03, 0xC7),
                (0x03, 0xC8),
                (0x03, 0xDF),
                (0x03, 0xE0),
                (0x03, 0xE1),
                (0x03, 0xE2),
                (0x03, 0xE3),
                (0x03, 0xE4),
                (0x03, 0xE5),
                (0x03, 0xE7),
                (0x03, 0xE8),
                (0x03, 0xE9),
                (0x04, 0x8D),
                (0x04, 0x8E),
                (0x04, 0x8F),
                (0x04, 0x90),
                (0x04, 0x91),
                (0x04, 0x94),
                (0x04, 0x95),
                (0x04, 0x97),
                (0x04, 0x9A),
                (0x04, 0x9B),
                (0x04, 0x9C),
                (0x04, 0x9D),
                (0x04, 0x9E),
                (0x04, 0xA0),
                (0x04, 0xA2),
                (0x04, 0xA9),
                (0x04, 0xAA),
                (0x04, 0xAB),
                (0x04, 0xAD),
                (0x04, 0xB1),
                (0x04, 0xB2),
                (0x04, 0xB3),
                (0x04, 0xBA),
                (0x04, 0xBB),
                (0x05, 0xD6),
                (0x05, 0xE8),
                (0x05, 0xED),
                (0x05, 0xEE),
                (0x05, 0xFC),
                (0x05, 0xFD),
                (0x06, 0xE2),
                (0x06, 0xE3),
                (0x06, 0xF2),
                (0x06, 0xF7),
                (0x06, 0xF9),
                (0x06, 0xFA),
                (0x06, 0xFB),
                (0x06, 0xFC),
                (0x06, 0xFD),
                (0x07, 0xE7),
                (0x07, 0xF4),
                (0x07, 0xF5),
                (0x07, 0xF9),
                (0x07, 0xFA),
                (0x07, 0xFB),
                (0x07, 0xFD),
                (0x08, 0xFD),
                (0x09, 0xF1),
                (0x09, 0xF3),
                (0x09, 0xF8),
                (0x09, 0xF9),
                (0x09, 0xFB),
                (0x09, 0xFC),
                (0x0B, 0xF2),
                (0x0B, 0xF5),
                (0x45, 0x51),
                (0x45, 0x92),
                (0x58, 0x70),
                (0x5F, 0xB0),
                (0x8B, 0x91),
                (0xB8, 0x80),
                (0xF8, 0xF8),
            ],
            Self::Mine => &[(0x07, 0xF1)],
            Self::Castle => &[
                (0x04, 0x8A),
                (0x04, 0x8C),
                (0x08, 0xC2),
                (0x09, 0xC5),
                (0x09, 0xC6),
                (0x09, 0xC8),
                (0x0A, 0xC5),
                (0x0B, 0xF0),
                (0x0B, 0xF1),
            ],
            Self::Tomb => &[
                (0x04, 0x87),
                (0x06, 0xC1),
                (0x07, 0xC4),
                (0x07, 0xC5),
                (0x07, 0xC6),
                (0x07, 0xC7),
                (0x07, 0xC8),
                (0x08, 0xC7),
                (0x08, 0xC8),
                (0x09, 0xC0),
                (0x0A, 0xC6),
            ],
            Self::Fancy => &[
                (0x03, 0xD8),
                (0x03, 0xD9),
                (0x03, 0xDA),
                (0x03, 0xDB),
                (0x03, 0xDC),
                (0x03, 0xDD),
                (0x06, 0xC0),
                (0x06, 0xC3),
                (0x06, 0xC5),
                (0x06, 0xC6),
                (0x06, 0xC7),
                (0x07, 0xBA),
                (0x07, 0xBB),
                (0x07, 0xBC),
                (0x07, 0xBD),
                (0x07, 0xBE),
                (0x07, 0xBF),
                (0x07, 0xC0),
                (0x07, 0xC1),
                (0x07, 0xC2),
                (0x08, 0xC1),
                (0x08, 0xC3),
                (0x08, 0xC4),
                (0x08, 0xC6),
                (0x09, 0xBD),
                (0x09, 0xBE),
                (0x09, 0xBF),
            ],
            Self::Cave => &[
                (0x00, 0x5D),
                (0x01, 0xBF),
                (0x03, 0xE6),
                (0x04, 0x88),
                (0x04, 0x9F),
                (0x04, 0xA1),
                (0x04, 0xAF),
                (0x04, 0xB0),
                (0x05, 0xF0),
                (0x06, 0xC2),
                (0x06, 0xC4),
                (0x06, 0xC8),
                (0x06, 0xDD),
                (0x06, 0xDF),
                (0x06, 0xE0),
                (0x06, 0xE1),
                (0x06, 0xE5),
                (0x06, 0xE6),
                (0x06, 0xEB),
                (0x06, 0xEC),
                (0x06, 0xEE),
                (0x06, 0xEF),
                (0x06, 0xF1),
                (0x06, 0xF3),
                (0x06, 0xF4),
                (0x06, 0xF5),
                (0x07, 0xB8),
                (0x07, 0xC3),
                (0x07, 0xE8),
                (0x07, 0xE9),
                (0x07, 0xEA),
                (0x07, 0xEB),
                (0x07, 0xEC),
                (0x07, 0xED),
                (0x07, 0xEF),
                (0x07, 0xF0),
                (0x07, 0xF2),
                (0x07, 0xF3),
                (0x08, 0xC5),
                (0x09, 0xC3),
                (0x09, 0xC7),
                (0x09, 0xF2),
                (0x09, 0xF5),
                (0x09, 0xFA),
                (0x09, 0xFD),
                (0x0A, 0xC4),
                (0x0A, 0xC7),
                (0x0A, 0xC8),
                (0x0A, 0xFC),
                (0x0B, 0xF3),
                (0x32, 0x4E),
                (0x3F, 0x9E),
                (0x66, 0xA9),
                (0x68, 0x88),
                (0x69, 0x67),
                (0x69, 0xB6),
                (0x70, 0x97),
                (0x76, 0xAE),
                (0x7C, 0xB9),
                (0x85, 0x8D),
                (0x95, 0x9E),
                (0x98, 0xA6),
                (0x99, 0xC4),
                (0x9A, 0xB5),
                (0xD3, 0x9D),
                (0xDA, 0xAC),
                (0xDE, 0xA8),
                (0xE1, 0xA8),
                (0xF1, 0x8F),
            ],
        }
    }

    /// Its looks: each one's name and the appearance it is (none: the kit's own materials).
    #[must_use]
    pub const fn looks(self) -> &'static [(&'static str, Option<u32>)] {
        match self {
            Self::Basic => &[
                ("own", None),
                ("plain", Some(0x4100_000E)),
                ("haunted", Some(0x4100_000F)),
                ("emplite", Some(0x4100_001F)),
            ],
            Self::Fancy => &[("own", None), ("broken", Some(0x4100_002C))],
            Self::Tomb | Self::Mine => &[("own", None)],
            Self::Castle => &[("own", None), ("human", Some(0x4100_002B))],
            Self::Cave => &[
                ("own", None),
                ("coldstone", Some(0x4100_0008)),
                ("seashore", Some(0x4100_0009)),
                ("greystone", Some(0x4100_000A)),
                ("ice", Some(0x4100_0018)),
                ("olthoi", Some(0x4100_0026)),
                ("red", Some(0x4100_002D)),
                ("dark", Some(0x4100_002F)),
            ],
        }
    }
}

/// Every piece of the kits: its kit, its cell mesh in the install, and what it is.
pub const PIECES: &[(Kit, u32, &str)] = &[
    // Who comes in by portal arrives here first: offered first.
    (Kit::Basic, 0x0100_016E, "entrance hall"),
    (Kit::Basic, 0x0100_003E, "turn"),
    (Kit::Basic, 0x0100_003F, "end"),
    (Kit::Basic, 0x0100_0040, "turn down 6 m"),
    (Kit::Basic, 0x0100_0041, "tee"),
    (Kit::Basic, 0x0100_0042, "straight"),
    (Kit::Basic, 0x0100_0047, "room floor side doorway"),
    (Kit::Basic, 0x0100_0048, "room floor corner"),
    (Kit::Basic, 0x0100_0049, "room floor side"),
    (Kit::Basic, 0x0100_004A, "room floor center"),
    (Kit::Basic, 0x0100_004B, "room floor inner corner"),
    (Kit::Basic, 0x0100_004C, "room middle side"),
    (Kit::Basic, 0x0100_004D, "room middle corner"),
    (Kit::Basic, 0x0100_004E, "room middle inner corner"),
    (Kit::Basic, 0x0100_004F, "room middle open"),
    (Kit::Basic, 0x0100_0050, "cross"),
    (Kit::Basic, 0x0100_0051, "straight wide"),
    (Kit::Basic, 0x0100_0052, "straight up 3 m"),
    (Kit::Basic, 0x0100_0053, "turn down 3 m"),
    (Kit::Basic, 0x0100_0054, "room middle side doorway"),
    (Kit::Basic, 0x0100_0055, "room ceiling side"),
    (Kit::Basic, 0x0100_0056, "room ceiling corner"),
    (Kit::Basic, 0x0100_0057, "room ceiling center"),
    (Kit::Basic, 0x0100_0058, "room ceiling inner corner"),
    (Kit::Basic, 0x0100_0072, "turn down 3 m left"),
    (Kit::Basic, 0x0100_0075, "turn down 6 m left"),
    (Kit::Basic, 0x0100_0088, "room floor center corner"),
    (Kit::Basic, 0x0100_0089, "room floor center side"),
    (Kit::Basic, 0x0100_0098, "room floor inner diagonal"),
    (Kit::Basic, 0x0100_0099, "room ceiling inner diagonal"),
    (Kit::Basic, 0x0100_009A, "room flat ceiling"),
    (Kit::Basic, 0x0100_009B, "room flat ceiling 2"),
    (Kit::Basic, 0x0100_009C, "room floor diagonal"),
    (Kit::Basic, 0x0100_009D, "room ceiling diagonal"),
    (Kit::Basic, 0x0100_00A4, "room middle diagonal"),
    (Kit::Basic, 0x0100_00A9, "straight up 1 m"),
    (Kit::Basic, 0x0100_00AA, "room ceiling diagonal 2"),
    (Kit::Basic, 0x0100_00AB, "room floor center inner corner"),
    (Kit::Basic, 0x0100_00AC, "room middle inner diagonal"),
    (Kit::Basic, 0x0100_00B8, "straight 10 m"),
    (Kit::Fancy, 0x0100_00BB, "hall turn down right 6 m"),
    (Kit::Fancy, 0x0100_00BC, "room floor doorway"),
    (Kit::Fancy, 0x0100_00BD, "room floor corner"),
    (Kit::Fancy, 0x0100_00BE, "room floor side"),
    (Kit::Fancy, 0x0100_00BF, "room floor inner diagonal"),
    (Kit::Fancy, 0x0100_00C0, "room ceiling corner"),
    (Kit::Fancy, 0x0100_00C1, "room ceiling side"),
    (Kit::Fancy, 0x0100_00C2, "room ceiling inner diagonal"),
    (Kit::Fancy, 0x0100_00C3, "hall straight"),
    (Kit::Fancy, 0x0100_00C4, "room floor diagonal"),
    (Kit::Fancy, 0x0100_00C5, "room ceiling diagonal"),
    (Kit::Fancy, 0x0100_00C6, "room ceiling center"),
    (Kit::Fancy, 0x0100_00C7, "hall straight 2"),
    (Kit::Fancy, 0x0100_00C8, "room floor inner corner"),
    (Kit::Fancy, 0x0100_00C9, "room ceiling inner corner"),
    (Kit::Fancy, 0x0100_00CA, "room floor center"),
    (Kit::Fancy, 0x0100_00CB, "room middle center side"),
    (Kit::Fancy, 0x0100_00CC, "room middle corner"),
    (Kit::Fancy, 0x0100_00CD, "hall turn"),
    (Kit::Fancy, 0x0100_00CE, "hall turn down left 3 m"),
    (Kit::Fancy, 0x0100_00CF, "hall straight up 3 m"),
    (Kit::Fancy, 0x0100_00D0, "room middle corner 2"),
    (Kit::Fancy, 0x0100_00D1, "room middle side"),
    (Kit::Fancy, 0x0100_00D2, "room middle open"),
    (Kit::Fancy, 0x0100_00D3, "room middle doorway"),
    (Kit::Fancy, 0x0100_00D4, "hall turn down left 6 m"),
    (Kit::Fancy, 0x0100_00D5, "hall end"),
    (Kit::Fancy, 0x0100_00D6, "room floor center 2"),
    (Kit::Fancy, 0x0100_00D7, "hall cross"),
    (Kit::Fancy, 0x0100_0117, "room middle inner diagonal"),
    (Kit::Fancy, 0x0100_0118, "room middle diagonal"),
    (Kit::Fancy, 0x0100_0139, "hall tee"),
    (Kit::Fancy, 0x0100_013A, "hall way out"),
    (Kit::Fancy, 0x0100_013B, "hall turn down right 3 m"),
    (Kit::Fancy, 0x0100_013C, "hall straight up 1 m"),
    (Kit::Tomb, 0x0100_00E5, "ramp"),
    (Kit::Tomb, 0x0100_00E6, "ramp turn plain"),
    (Kit::Tomb, 0x0100_00E7, "ramp turn"),
    (Kit::Tomb, 0x0100_00E8, "ramp turn back"),
    (Kit::Tomb, 0x0100_00E9, "straight"),
    (Kit::Tomb, 0x0100_00EA, "straight long"),
    (Kit::Tomb, 0x0100_00EB, "room floor entrance"),
    (Kit::Tomb, 0x0100_00EC, "room floor open"),
    (Kit::Tomb, 0x0100_00ED, "room floor side"),
    (Kit::Tomb, 0x0100_00EE, "room floor round corner"),
    (Kit::Tomb, 0x0100_00EF, "room roof round corner"),
    (Kit::Tomb, 0x0100_00F0, "room roof side"),
    (Kit::Tomb, 0x0100_00F1, "room roof open"),
    (Kit::Tomb, 0x0100_00F6, "end"),
    (Kit::Tomb, 0x0100_00F7, "straight medium"),
    (Kit::Tomb, 0x0100_0119, "cross"),
    (Kit::Tomb, 0x0100_011A, "tee"),
    (Kit::Tomb, 0x0100_011B, "corner"),
    (Kit::Tomb, 0x0100_011C, "end room"),
    (Kit::Tomb, 0x0100_011D, "end wall to earth"),
    (Kit::Tomb, 0x0100_011E, "ramp plain"),
    (Kit::Tomb, 0x0100_011F, "room floor corner"),
    (Kit::Tomb, 0x0100_0120, "room middle side"),
    (Kit::Tomb, 0x0100_0121, "room roof corner"),
    (Kit::Tomb, 0x0100_0122, "room middle corner"),
    (Kit::Tomb, 0x0100_0123, "room floor diagonal corner"),
    (Kit::Tomb, 0x0100_0124, "room floor three sides"),
    (Kit::Tomb, 0x0100_0125, "room middle diagonal corner"),
    (Kit::Tomb, 0x0100_0126, "room roof diagonal corner"),
    (Kit::Tomb, 0x0100_0127, "room middle open"),
    (Kit::Tomb, 0x0100_0128, "room middle round corner"),
    (Kit::Tomb, 0x0100_0129, "room roof one corner"),
    (Kit::Tomb, 0x0100_012A, "room middle three sides"),
    (Kit::Tomb, 0x0100_012B, "room roof three sides"),
    (Kit::Tomb, 0x0100_012C, "room roof two corners"),
    (Kit::Tomb, 0x0100_014D, "room floor entrance 2"),
    (Kit::Tomb, 0x0100_014E, "room middle two sides"),
    (Kit::Tomb, 0x0100_014F, "room floor two sides"),
    (Kit::Tomb, 0x0100_0150, "room roof two sides"),
    (Kit::Tomb, 0x0100_0153, "straight wall to earth"),
    (Kit::Tomb, 0x0100_0154, "straight earth"),
    (Kit::Tomb, 0x0100_0155, "ramp turn back plain"),
    (Kit::Tomb, 0x0100_0169, "end earth"),
    (Kit::Mine, 0x0100_00B2, "straight"),
    (Kit::Mine, 0x0100_00B1, "turn"),
    (Kit::Mine, 0x0100_00B4, "bend"),
    (Kit::Mine, 0x0100_00AF, "tee"),
    (Kit::Mine, 0x0100_00B0, "tee 2"),
    (Kit::Mine, 0x0100_00AD, "cross"),
    (Kit::Mine, 0x0100_00A6, "end"),
    (Kit::Mine, 0x0100_00A8, "end 2"),
    (Kit::Mine, 0x0100_00B3, "end left"),
    (Kit::Mine, 0x0100_00B9, "end tee"),
    (Kit::Mine, 0x0100_00AE, "down half"),
    (Kit::Mine, 0x0100_00A5, "drop"),
    // Where a mine meets a basic hall, and a mine's mouth.
    (Kit::Mine, 0x0100_00A7, "to basic"),
    (Kit::Mine, 0x0100_016F, "mouth"),
    (Kit::Castle, 0x0100_00F8, "end a"),
    (Kit::Castle, 0x0100_00F9, "plain straight"),
    (Kit::Castle, 0x0100_00FB, "plain cross"),
    (Kit::Castle, 0x0100_00FC, "plain room a"),
    (Kit::Castle, 0x0100_00FD, "plain tee"),
    (Kit::Castle, 0x0100_00FE, "plain end c"),
    (Kit::Castle, 0x0100_00FF, "end a dressed"),
    (Kit::Castle, 0x0100_0100, "cross"),
    (Kit::Castle, 0x0100_0101, "end c dressed"),
    (Kit::Castle, 0x0100_0102, "straight"),
    (Kit::Castle, 0x0100_0103, "corner"),
    (Kit::Castle, 0x0100_0104, "room a dressed"),
    (Kit::Castle, 0x0100_0105, "end b dressed"),
    (Kit::Castle, 0x0100_0106, "tee"),
    (Kit::Castle, 0x0100_0107, "ramp stairs"),
    (Kit::Castle, 0x0100_0108, "ramp turn stairs"),
    (Kit::Castle, 0x0100_0144, "end dressed"),
    (Kit::Castle, 0x0100_0145, "room b dressed"),
    (Kit::Castle, 0x0100_0146, "tower top doorless"),
    (Kit::Castle, 0x0100_0147, "tower mid a"),
    (Kit::Castle, 0x0100_0148, "tower mid b"),
    (Kit::Castle, 0x0100_0149, "tower mid doorless"),
    (Kit::Castle, 0x0100_014A, "tower mid c"),
    (Kit::Castle, 0x0100_014B, "tower bottom a"),
    (Kit::Castle, 0x0100_0152, "room c dressed"),
    (Kit::Castle, 0x0100_0156, "plain ramp turn"),
    (Kit::Castle, 0x0100_0157, "plain ramp turn back"),
    (Kit::Castle, 0x0100_0158, "plain ramp turn back stairs"),
    (Kit::Castle, 0x0100_0159, "plain ramp turn stairs"),
    (Kit::Castle, 0x0100_015A, "plain room a dressed"),
    (Kit::Castle, 0x0100_015B, "plain end a dressed"),
    (Kit::Castle, 0x0100_015C, "plain end"),
    (Kit::Castle, 0x0100_015D, "plain end b"),
    (Kit::Castle, 0x0100_015E, "plain end a"),
    (Kit::Castle, 0x0100_015F, "plain ramp stairs"),
    (Kit::Castle, 0x0100_0160, "ramp turn back stairs"),
    (Kit::Castle, 0x0100_0161, "end c"),
    (Kit::Castle, 0x0100_0162, "tower bottom c"),
    (Kit::Castle, 0x0100_0163, "tower top a"),
    (Kit::Castle, 0x0100_0164, "tower bottom b"),
    (Kit::Castle, 0x0100_0165, "tower top c"),
    (Kit::Castle, 0x0100_0166, "room b"),
    (Kit::Castle, 0x0100_0167, "plain room b"),
    (Kit::Castle, 0x0100_0168, "room a"),
    (Kit::Castle, 0x0100_016C, "ramp"),
    // Where a castle meets a tomb's hall, and a fancy hall.
    (Kit::Castle, 0x0100_0109, "to tomb"),
    (Kit::Castle, 0x0100_0143, "to fancy"),
    // The cave's: tunnels of 20 m, small (10 m), large (30 m) and wide; those between sizes; its
    // second set (`a`); and where a tomb's hall meets a cave. Its entrance from the land and its
    // two great rooms are not among them.
    (Kit::Cave, 0x0100_001C, "straight"),
    (Kit::Cave, 0x0100_001D, "cross"),
    (Kit::Cave, 0x0100_001E, "turn"),
    (Kit::Cave, 0x0100_001F, "up"),
    (Kit::Cave, 0x0100_0020, "straight up"),
    (Kit::Cave, 0x0100_0021, "end"),
    (Kit::Cave, 0x0100_0022, "wide cross 2"),
    (Kit::Cave, 0x0100_0023, "wide turn"),
    (Kit::Cave, 0x0100_0024, "wide straight up"),
    (Kit::Cave, 0x0100_0025, "wide end"),
    (Kit::Cave, 0x0100_0026, "wide straight"),
    (Kit::Cave, 0x0100_0027, "bend up 45"),
    (Kit::Cave, 0x0100_0028, "bend 45"),
    (Kit::Cave, 0x0100_0029, "diagonal"),
    (Kit::Cave, 0x0100_002A, "straight bend"),
    (Kit::Cave, 0x0100_002E, "to large straight bend"),
    (Kit::Cave, 0x0100_002F, "bend up 45 right"),
    (Kit::Cave, 0x0100_0030, "y down"),
    (Kit::Cave, 0x0100_0031, "straight bend bulge"),
    (Kit::Cave, 0x0100_0032, "straight up bend"),
    (Kit::Cave, 0x0100_0033, "to small straight bend"),
    (Kit::Cave, 0x0100_0034, "small y down"),
    (Kit::Cave, 0x0100_0036, "large end"),
    (Kit::Cave, 0x0100_0037, "small straight up bend"),
    (Kit::Cave, 0x0100_003D, "wide cross"),
    (Kit::Cave, 0x0100_0044, "to large straight"),
    (Kit::Cave, 0x0100_0045, "wide large cross 2"),
    (Kit::Cave, 0x0100_0046, "wide large end"),
    (Kit::Cave, 0x0100_0059, "small end"),
    (Kit::Cave, 0x0100_005A, "small bend up 45 right"),
    (Kit::Cave, 0x0100_005B, "small diagonal right"),
    (Kit::Cave, 0x0100_005C, "small bend up 45"),
    (Kit::Cave, 0x0100_005D, "small straight up bend right"),
    (Kit::Cave, 0x0100_005E, "small transition straight up"),
    (Kit::Cave, 0x0100_005F, "small straight up"),
    (Kit::Cave, 0x0100_0060, "diagonal right"),
    (Kit::Cave, 0x0100_0061, "transition straight up"),
    (Kit::Cave, 0x0100_0062, "bend up"),
    (Kit::Cave, 0x0100_0063, "small t down"),
    (Kit::Cave, 0x0100_0064, "wide cross 3"),
    (Kit::Cave, 0x0100_0065, "t"),
    (Kit::Cave, 0x0100_0066, "straight bulge"),
    (Kit::Cave, 0x0100_0067, "y"),
    (Kit::Cave, 0x0100_006B, "wide small cross 2"),
    (Kit::Cave, 0x0100_006C, "wide small end"),
    (Kit::Cave, 0x0100_006D, "small bend 45"),
    (Kit::Cave, 0x0100_006E, "small transition straight bend"),
    (Kit::Cave, 0x0100_006F, "small cross"),
    (Kit::Cave, 0x0100_0070, "small y"),
    (Kit::Cave, 0x0100_0071, "small t"),
    (Kit::Cave, 0x0100_0073, "t down"),
    (Kit::Cave, 0x0100_0074, "straight up bend right"),
    (Kit::Cave, 0x0100_0076, "to small straight"),
    (Kit::Cave, 0x0100_0077, "small diagonal"),
    (Kit::Cave, 0x0100_0078, "small straight"),
    (Kit::Cave, 0x0100_0079, "wide large cross 3"),
    (Kit::Cave, 0x0100_007A, "wide large turn"),
    (Kit::Cave, 0x0100_007B, "large diagonal"),
    (Kit::Cave, 0x0100_007C, "wide large cross"),
    (Kit::Cave, 0x0100_007D, "wide large straight"),
    (Kit::Cave, 0x0100_007E, "large bend up 45 right"),
    (Kit::Cave, 0x0100_007F, "large cross"),
    (Kit::Cave, 0x0100_0080, "large straight up"),
    (Kit::Cave, 0x0100_0081, "large t"),
    (Kit::Cave, 0x0100_008B, "large t down"),
    (Kit::Cave, 0x0100_00A1, "large y down"),
    (Kit::Cave, 0x0100_00A2, "large y"),
    (Kit::Cave, 0x0100_00A3, "large straight up bend"),
    (Kit::Cave, 0x0100_00BA, "transition straight bend"),
    (Kit::Cave, 0x0100_00D8, "large straight up bend right"),
    (Kit::Cave, 0x0100_00D9, "large transition straight up"),
    (Kit::Cave, 0x0100_00DA, "large straight"),
    (Kit::Cave, 0x0100_00DB, "large bend up 45"),
    (Kit::Cave, 0x0100_00DD, "wide large straight up"),
    (Kit::Cave, 0x0100_00DE, "large turn"),
    (Kit::Cave, 0x0100_00DF, "small turn"),
    (Kit::Cave, 0x0100_00E0, "shallow end a"),
    (Kit::Cave, 0x0100_00E2, "straight bend a"),
    (Kit::Cave, 0x0100_00E3, "bend 45 a"),
    (Kit::Cave, 0x0100_00E4, "to tomb"),
    (Kit::Cave, 0x0100_00F2, "straight a"),
    (Kit::Cave, 0x0100_00F3, "turn a"),
    (Kit::Cave, 0x0100_00F4, "t a"),
    (Kit::Cave, 0x0100_00F5, "end a"),
    (Kit::Cave, 0x0100_010A, "small straight bend"),
    (Kit::Cave, 0x0100_010B, "wide small straight"),
    (Kit::Cave, 0x0100_010C, "wide small turn"),
    (Kit::Cave, 0x0100_010D, "large bend up"),
    (Kit::Cave, 0x0100_010E, "large bend 45"),
    (Kit::Cave, 0x0100_010F, "large diagonal right"),
    (Kit::Cave, 0x0100_012D, "straight up a"),
    (Kit::Cave, 0x0100_012E, "t down a"),
    (Kit::Cave, 0x0100_012F, "cross a"),
    (Kit::Cave, 0x0100_0130, "y down 45 a"),
    (Kit::Cave, 0x0100_0131, "up a"),
    (Kit::Cave, 0x0100_0132, "y down a"),
    (Kit::Cave, 0x0100_0133, "y a"),
    (Kit::Cave, 0x0100_0134, "diagonal a"),
    (Kit::Cave, 0x0100_0136, "bend up 45 right a"),
    (Kit::Cave, 0x0100_0137, "bend up 45 a"),
    (Kit::Cave, 0x0100_0138, "bend up a"),
    (Kit::Cave, 0x0100_013D, "small bend up"),
    (Kit::Cave, 0x0100_013E, "large straight bend"),
    (Kit::Cave, 0x0100_013F, "wide small cross 3"),
    (Kit::Cave, 0x0100_0140, "wide small cross"),
    (Kit::Cave, 0x0100_0141, "wide small straight up"),
    (Kit::Cave, 0x0100_014C, "y straight a"),
    (Kit::Cave, 0x0100_0151, "transition straight"),
];

/// The kit pieces that reach past their doorways, by cell mesh, and how far, in meters: a little
/// into the piece joined there, as AC2's own dungeons lay them. A piece that does is fitted with
/// another sharing that much of its space ([`crate::room::Shape::reaching`]); every other piece of
/// every kit, and every retail piece, reaches no further than its doorways.
pub const REACH: &[(u32, f32)] = &[
    // The castle's piece to a tomb's hall reaches 0.84 m into the hall.
    (0x0100_0109, 0.84),
];

/// How far the kit piece of cell mesh `mesh` reaches past its doorways ([`REACH`]), in meters.
#[must_use]
pub fn reach(mesh: u32) -> f32 {
    REACH
        .iter()
        .find(|(m, _)| *m == mesh)
        .map_or(0.0, |(_, r)| *r)
}

/// A kit's piece made in a world: its kit, its cell mesh and what it is, its environment, and
/// each of the kit's looks' surfaces for it, by the look's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ac2Piece {
    pub kit: Kit,
    pub mesh: u32,
    pub label: &'static str,
    pub environment: DataId,
    pub looks: Vec<(&'static str, Vec<DataId>)>,
}

/// The most lights a kit offers: the commonest in its dungeons.
pub const LIGHTS: usize = 8;

/// A light of a kit made in a world: its kit, its color (`0xAARRGGBB`) and reach (meters), the
/// setup a room's static stands for it with, and how many stand in the kit's AC2 dungeons.
#[derive(Clone, Debug, PartialEq)]
pub struct Ac2Light {
    pub kit: Kit,
    pub color: u32,
    pub reach: f32,
    pub setup: DataId,
    pub uses: usize,
}

impl Ac2Light {
    /// What a builder sees it called: its color and reach, `#FFB070, 12 m`.
    #[must_use]
    pub fn label(&self) -> String {
        format!("#{:06X}, {:.0} m", self.color & 0x00FF_FFFF, self.reach)
    }
}

/// The lights of `kits` made from `ac2`: for each kit, the [`LIGHTS`] its AC2 dungeons use most,
/// each a light setup, a new record of `out` with its id from `ids`.
///
/// # Errors
/// A setup that will not encode.
pub fn make_lights(
    ac2: &Ac2,
    kits: &BTreeSet<Kit>,
    ids: &mut dyn NewIds,
    out: &mut Records,
) -> Result<Vec<Ac2Light>, String> {
    let mut lights = Vec::new();
    for &kit in kits {
        for (color, reach, uses) in dereth_ac2_convert::light::dungeon_lights(ac2, kit.dungeons())
            .into_iter()
            .take(LIGHTS)
        {
            let setup = dereth_ac2_convert::light::light_setup(color, reach, ids, out)?;
            lights.push(Ac2Light {
                kit,
                color,
                reach,
                setup,
                uses,
            });
        }
    }
    Ok(lights)
}

/// Every piece of `kits` made from `ac2`, each a new record of `out` with its id from `ids`, in
/// the order of [`PIECES`]; a piece the install does not hold is left out.
///
/// # Errors
/// A picture that will not read.
pub fn make(
    ac2: &Ac2,
    kits: &BTreeSet<Kit>,
    ids: &mut dyn NewIds,
    out: &mut Records,
) -> Result<Vec<Ac2Piece>, String> {
    let wr = ac2.records();
    let mut pictures = Pictures::default();
    let mut made = std::collections::BTreeMap::new();
    let mut pieces = Vec::new();
    for &(kit, mesh, label) in PIECES.iter().filter(|(k, _, _)| kits.contains(k)) {
        let Some((environment, own, _, _)) = dereth_ac2_convert::cell::environment_by_material(
            wr,
            ac2,
            mesh,
            &mut pictures,
            ids,
            out,
            &mut made,
        )?
        else {
            continue;
        };
        let mut looks = vec![(kit.looks()[0].0, own)];
        for &(name, key) in &kit.looks()[1..] {
            if let Some(surfaces) = dereth_ac2_convert::cell::look_surfaces(
                wr,
                ac2,
                mesh,
                key,
                &mut pictures,
                ids,
                out,
                &mut made,
            )? {
                looks.push((name, surfaces));
            }
        }
        pieces.push(Ac2Piece {
            kit,
            mesh,
            label,
            environment,
            looks,
        });
    }
    Ok(pieces)
}

/// The file in a converted land's overlay folder naming the environment each AC2 cell mesh became:
/// a line per mesh and what it was made for, `mesh made_for environment surfaces`, tab-separated,
/// the ids in hex and the surfaces (its cells' list, in order) apart by spaces.
pub const LAND_TABLE: &str = "environments.tsv";

/// The environments a converted land made of AC2's cell meshes ([`LAND_TABLE`]): for each mesh,
/// the environment and the surfaces its cells name. A mesh made both for a building's rooms and
/// for the dungeons is the dungeons' here, as a kit's pieces are dungeon pieces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LandEnvironments(BTreeMap<u32, (DataId, Vec<DataId>)>);

impl LandEnvironments {
    /// The table in `text`.
    ///
    /// # Errors
    /// A line that is not a mesh, what it was made for, an environment and surfaces.
    pub fn parse(text: &str) -> Result<Self, String> {
        let hex = |s: &str, n: usize| {
            u32::from_str_radix(s.trim(), 16).map_err(|_| format!("line {n}: {s:?} is not an id"))
        };
        let mut rows: BTreeMap<u32, (bool, DataId, Vec<DataId>)> = BTreeMap::new();
        for (k, line) in text.lines().enumerate().skip(1) {
            let n = k + 1;
            if line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            let [mesh, made_for, env, surfaces] = cols[..] else {
                return Err(format!("line {n}: not four columns"));
            };
            let dungeon = made_for == "dungeon";
            let surfaces = surfaces
                .split_whitespace()
                .map(|s| hex(s, n).map(DataId))
                .collect::<Result<Vec<_>, _>>()?;
            let row = (dungeon, DataId(hex(env, n)?), surfaces);
            let mesh = hex(mesh, n)?;
            if rows.get(&mesh).is_none_or(|(had, ..)| dungeon && !had) {
                rows.insert(mesh, row);
            }
        }
        Ok(Self(
            rows.into_iter()
                .map(|(m, (_, env, surfaces))| (m, (env, surfaces)))
                .collect(),
        ))
    }

    /// The table of the world's land: the last of `imports` (overlay folders) that holds one.
    /// `None` when none does, as for a land not converted from AC2.
    ///
    /// # Errors
    /// A table that will not read or parse.
    pub fn of_imports(imports: &[PathBuf]) -> Result<Option<Self>, String> {
        for dir in imports.iter().rev() {
            let path = dir.join(LAND_TABLE);
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    return Self::parse(&text)
                        .map(Some)
                        .map_err(|e| format!("{}: {e}", path.display()));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
        }
        Ok(None)
    }

    /// The environment mesh `mesh` became and the surfaces its cells name.
    #[must_use]
    pub fn get(&self, mesh: u32) -> Option<&(DataId, Vec<DataId>)> {
        self.0.get(&mesh)
    }
}

/// Every piece of `kits` as the land `land` holds it, in the order of [`PIECES`]: the land's
/// environment for its mesh, and one look, the kit's own (`own`), the surfaces the land's cells
/// name. Answers the pieces, and those the land lacks (left out).
#[must_use]
pub fn from_land(
    land: &LandEnvironments,
    kits: &BTreeSet<Kit>,
) -> (Vec<Ac2Piece>, Vec<(Kit, u32, &'static str)>) {
    let mut pieces = Vec::new();
    let mut lacks = Vec::new();
    for &(kit, mesh, label) in PIECES.iter().filter(|(k, _, _)| kits.contains(k)) {
        match land.get(mesh) {
            Some((environment, surfaces)) => pieces.push(Ac2Piece {
                kit,
                mesh,
                label,
                environment: *environment,
                looks: vec![(kit.looks()[0].0, surfaces.clone())],
            }),
            None => lacks.push((kit, mesh, label)),
        }
    }
    (pieces, lacks)
}

/// A name a new record of a kit is made under, as a symbol: `ac2-` and its words and numbers,
/// lowercase, joined by hyphens (`surface 310002C2 …` is `ac2-surface-310002c2-…`).
#[must_use]
pub fn symbol(name: &str) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    format!("ac2-{}", words.join("-"))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: AC2's dungeon kits as a world's pieces)
    use super::*;

    #[test]
    fn each_kit_has_its_own_look_first_and_its_pieces_each_once() {
        for kit in Kit::ALL {
            assert_eq!(kit.looks()[0], ("own", None));
            assert_eq!(Kit::parse(kit.name()), Some(kit));
        }
        let meshes: BTreeSet<u32> = PIECES.iter().map(|p| p.1).collect();
        assert_eq!(meshes.len(), PIECES.len());
        let count = |k: Kit| PIECES.iter().filter(|p| p.0 == k).count();
        assert_eq!((count(Kit::Basic), count(Kit::Fancy)), (40, 35));
        let labels: BTreeSet<(Kit, &str)> = PIECES.iter().map(|p| (p.0, p.2)).collect();
        assert_eq!(
            labels.len(),
            PIECES.len(),
            "a label names one piece of its kit"
        );
    }

    /// A land's table gives each mesh's environment and surfaces, a dungeon's line over a
    /// building's for the same mesh in either order; a kit's pieces are the land's environments in
    /// one look, its own, and those the land lacks are named.
    #[test]
    fn a_lands_table_gives_the_kit_its_environments_dungeon_lines_first() {
        let text = "mesh\tmade_for\tenvironment\tsurfaces\n\
                    01000042\tbuilding\t0D00C001\t0800C001\n\
                    01000042\tdungeon\t0D00C002\t0800C002 0800C003\n\
                    0100003F\tdungeon\t0D00C003\t0800C004\n\
                    0100003F\tbuilding\t0D00C004\t0800C005\n\
                    0100003E\tbuilding\t0D00C005\t\n";
        let land = LandEnvironments::parse(text).unwrap();
        assert_eq!(
            land.get(0x0100_0042),
            Some(&(
                DataId(0x0D00_C002),
                vec![DataId(0x0800_C002), DataId(0x0800_C003)]
            ))
        );
        assert_eq!(land.get(0x0100_003F).unwrap().0, DataId(0x0D00_C003));
        assert_eq!(land.get(0x0100_003E), Some(&(DataId(0x0D00_C005), vec![])));
        let (pieces, lacks) = from_land(&land, &BTreeSet::from([Kit::Basic]));
        let labels: Vec<&str> = pieces.iter().map(|p| p.label).collect();
        assert_eq!(labels, ["turn", "end", "straight"]);
        let straight = &pieces[2];
        assert_eq!(straight.environment, DataId(0x0D00_C002));
        assert_eq!(
            straight.looks,
            [("own", land.get(0x0100_0042).unwrap().1.clone())]
        );
        assert_eq!(lacks.len(), 37);
        assert!(lacks.iter().all(|(k, ..)| *k == Kit::Basic));
        let e = LandEnvironments::parse("mesh\tmade_for\tenvironment\tsurfaces\n0100\tdungeon\n")
            .unwrap_err();
        assert!(e.contains("line 2"), "{e}");
    }

    #[test]
    fn a_record_name_is_a_symbol_of_its_words() {
        let s = symbol("surface 310002C2 (0, 1074790497, [0, 0, 0, 0]) true false");
        assert_eq!(s, "ac2-surface-310002c2-0-1074790497-0-0-0-0-true-false");
        assert!(dereth_pack::symbol::is_name(&s));
    }
}
