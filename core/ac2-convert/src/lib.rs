//! Asheron's Call 2's dungeon cells and materials in Asheron's Call's formats: a cell mesh as an
//! environment, a material as a surface and image texture, read from a player's own Asheron's Call
//! 2 files.
//!
//! What the AC2-in-AC1 world's converter and Lyceum's AC2 dungeon kit share.
//!
//! **Depends on** the AC2 records and their reader (`auberean-assets`, `auberean-dat`,
//! `auberean-world`, `auberean-primitives`), the AC1 records it writes (`dereth-assets`, `dereth-dat`), the cell trees a
//! new shape needs (`dereth-builder`), the pictures' decoder (`dereth-render-cpu`) and
//! `dereth-primitives`. **Used by** `dereth-art`'s `ac2-world` and Lyceum's Foundry.
//!
//! - [`source`]: the player's AC2 files, open, and what is read from them.
//! - [`records`]: the ids handed out, the records made, and AC2's pictures as AC1 images.
//! - [`surface`]: an AC2 material, with a look's changes, as an AC1 surface.
//! - [`cell`]: an AC2 cell mesh as an AC1 environment of one cell shape.
//! - [`light`]: AC2's dungeon lights as AC1's lights, and the lights a set of its dungeons use.
//!
//! **Must never** write a player's files: what it makes is records, handed back to the caller.

pub mod cell;
pub mod light;
pub mod records;
pub mod source;
pub mod surface;
