//! A world: the packs it is built from, in order, and where its build goes.
//!
//! ```toml
//! # world.toml
//! key = "sample-world"      # the world's key, which every overlay container records
//! game = "ac1"              # the game it is for: ac1, the default (a world of another game is refused)
//! era = "eor"               # the era whose files it is built on: eor (default) or infiltration
//! packs = ["sample"]        # the load order, folders relative to this file
//! imports = []              # overlay folders an import made, laid under the packs
//! out = "build"             # the default output folder, relative to this file
//! [ids]
//! wcid = 100000             # where new weenie class ids start
//!
//! [[override]]              # where two packs disagree, which wins ([`crate::conflict`])
//! pack = "holtburg-plus"
//! over = "sample"
//! items = ["weenie 1234"]   # these only; without a list, everything both set
//! ```
//!
//! The world's lockfile, `world.lock`, sits beside it ([`crate::lock`]).
//!
//! A world is built on the files of one era, which it names: the end of retail's
//! (`client_portal.dat` and its siblings) or Infiltration's (`portal.dat` and `cell.dat`, from
//! February 2005). It is built over those files and no other, its records are written in their
//! layout, and its server content goes over that era's world database.
//!
//! The world file is the shared pack format's (`dereth_pack::world`), read in Lyceum's dialect
//! ([`crate::dialect::Ac1`]): its eras are the two above, and `[ids]` can start the weenie class
//! ids elsewhere (`wcid`).

pub use dereth_primitives::EraId;

/// Where new weenie class ids start unless the world says: above every class id of the
/// end-of-retail world database (its highest is 90,273).
pub const DEFAULT_WCID_START: u32 = 100_000;

/// A world, read.
pub type World = dereth_pack::World<crate::dialect::Ac1>;
