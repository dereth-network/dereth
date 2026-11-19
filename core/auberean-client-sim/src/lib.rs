//! The world without a window: the land, the interior cells and the solid objects about the player
//! as physics finds them, the player's body moved through them by its controls and reported
//! through the client, and the bodies of everything else the client knows.
//!
//! **Depends on** the client (`auberean-client-runtime`, `auberean-client-model`,
//! `auberean-client-contract`, `auberean-protocol`: its model, the body it reports, its actions and
//! controls), the world (`auberean-world`, `auberean-assets`, `auberean-dat`), bodies
//! (`auberean-physics`, `auberean-animation`), `auberean-primitives` and `dereth-primitives`.
//! **Used by** the SDK (`auberean-client-sdk`, which hands it out as `sim`), and through it the
//! headless client and the window's scene, which draws what it holds.
//!
//! - [`ground`]: the world's physics about the player: the near window of landblocks
//!   ([`near`]), the interior cells about the player's block, the cells asked of the client's data
//!   cache ([`cells`]), and the table every body moves against.
//! - [`player`]: the player's body: made where the server put it, stood on the ground once the
//!   ground there is in, steered by the player's controls, moved in ticks of a tenth of a second
//!   against the ground, its motions and jumps reported through the client; and its figure, whose
//!   animation sets off the jumps it leads into.
//! - [`others`]: the bodies of everything else the client knows in the world, steered by the
//!   positions the server sends for them and standing in the table the player's body moves
//!   against; [`notices`]: what a body asks of its own figure after its ticks; [`events`]: what
//!   the server asks of the bodies, and the moves it orders the player's body to make.
//! - [`walk`]: these put together for a program without a window, one frame at a time, in the
//!   order the window's scene runs them.
//! - [`orbit`]: the orbit camera an interface may choose in place of the game's own, and the
//!   player's movement under it, camera- or character-based; the window's scene and [`walk`]
//!   both steer the body through it.
//! - [`figures`]: an object's look, from the visual description it was sent; [`profile`]: what an
//!   object's qualities make of its body.
//!
//! **Must never** reach a device, a window, a socket or a clock, nor anything that draws: time
//! arrives from the host, and the controls as the contract's.

pub mod cells;
pub mod events;
pub mod figures;
pub mod ground;
pub mod near;
pub mod notices;
pub mod orbit;
pub mod others;
pub mod player;
pub mod profile;
pub mod walk;

use auberean_world::records::{RegionSet, WorldRecords};

pub use ground::Ground;
pub use orbit::OrbitCamera;
pub use player::Player;
pub use walk::Walk;

/// A block's side, meters.
pub const BLOCK_LENGTH: f32 = 160.0;
/// The lowest cell index of an interior cell; land cells are 1 to 64.
pub const FIRST_INTERIOR: u32 = 0x100;

/// The world's records and its region, read once.
pub struct WorldData {
    pub records: WorldRecords,
    pub region: RegionSet,
}

impl core::fmt::Debug for WorldData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WorldData").finish_non_exhaustive()
    }
}

impl WorldData {
    /// The world of `records`.
    ///
    /// # Errors
    /// The region does not read.
    pub fn new(records: WorldRecords) -> Result<Self, String> {
        let region = records.region().map_err(|e| format!("{e:?}"))?;
        Ok(Self { records, region })
    }
}

/// Whether a cell id names an interior cell.
#[must_use]
pub fn is_interior(cell: u32) -> bool {
    cell & 0xFFFF >= FIRST_INTERIOR
}
