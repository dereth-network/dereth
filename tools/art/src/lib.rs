//! Surveys the models the world is built from: where each one is used, how often, what it is
//! made of, and a picture of it; packs an art pack's folder into one file; and builds the AC2 look
//! from a player's own Asheron's Call 2 files.
//!
//! **Depends on** `dereth-primitives`, `dereth-dat` and `dereth-assets` (the retail files and
//! their records), `dereth-world-data` (the interior cells and the buildings they belong to),
//! `dereth-terrain` (the landblock mesh), `dereth-client-runtime` (a landblock's outdoor contents
//! placed as the client places them, and the models' parts and triangles), `dereth-scene` (the
//! pictures, decoded as the client decodes them, and an art pack opened and packed as the client
//! opens it, with no device), `dereth-render-cpu` (the surface record's type bits and the
//! block-compressed pictures' texels), `auberean-assets`, `auberean-dat` and `auberean-world`
//! (a player's Asheron's Call 2 install, its records and its region, for the AC2 look and the
//! AC2-in-AC1 world); and for that world `auberean-animation` (AC2's placed objects that play,
//! posed as AC2 poses them), `auberean-text` (its map labels' words), `auberean-primitives` (its
//! shapes' coordinates), `dereth-builder` (the trees a new model or room needs), `dereth-physics`
//! (the starting area's ground and water, told as the server tells them), `dereth-landscape`
//! (the scenery the converted land grows, placed as the client places it) and `auberean-draw` (its
//! sky's materials made ready as AC2 draws them, to work out their pictures). **Used
//! by** nothing: it is the `dereth-art` program, which comes beside the client, and which the
//! launcher runs to build the AC2 look.
//!
//! **Must never** write anywhere but the output folder or packed file it is given (and, while it
//! builds the AC2 look, a folder beside that file), or depend on a
//! server crate: the server's spawns arrive as a table the server's own importer writes.
//!
//! [`pack`] packs an art pack's folder as the client opens it. [`skeleton`] writes a setup as a
//! skeleton an artist rigs a new body to, with its own parts as a reference body and its animations as
//! clips. [`ac2_look`] builds the AC2 look, Asheron's Call 2's ground, trees, rocks and sky, as a
//! pack from the player's own Asheron's Call 2 files and the tables of which AC2 thing stands for
//! which of this game's.
//!
//! [`survey::run`] is the whole survey: [`world`] walks every landblock into a [`tally::Tally`],
//! [`character`] adds the bodies and the clothing, [`spawns`] reads the server's table,
//! [`model`] says what each model is made of, [`thumb`] draws its picture, [`pictures`] says what
//! each retail picture is drawn on and [`output`] writes the tables, the summary and the report.

#![doc(html_no_source)]

pub mod ac2_look;
pub mod ac2_world;
pub mod character;
pub mod model;
pub mod output;
pub mod pack;
pub mod pictures;
pub mod skeleton;
pub mod spawns;
pub mod survey;
pub mod tally;
pub mod thumb;
pub mod world;

#[cfg(test)]
mod tests;
