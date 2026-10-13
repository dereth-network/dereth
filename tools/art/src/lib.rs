//! Surveys the models the world is built from: where each one is used, how often, what it is
//! made of, and a picture of it.
//!
//! **Depends on** `dereth-primitives`, `dereth-dat` and `dereth-assets` (the retail files and
//! their records), `dereth-world-data` (the interior cells and the buildings they belong to),
//! `dereth-terrain` (the landblock mesh), `dereth-client-runtime` (a landblock's outdoor contents
//! placed as the client places them, and the models' parts and triangles), `dereth-scene` (the
//! pictures, decoded as the client decodes them, with no device) and `dereth-render-cpu` (the
//! surface record's type bits). **Used by** nothing: it is the `dereth-art` program.
//!
//! **Must never** write anywhere but the output folder it is given, or depend on a server crate:
//! the server's spawns arrive as a table the server's own importer writes.
//!
//! [`survey::run`] is the whole survey: [`world`] walks every landblock into a [`tally::Tally`],
//! [`character`] adds the bodies and the clothing, [`spawns`] reads the server's table,
//! [`model`] says what each model is made of, [`thumb`] draws its picture and [`output`] writes
//! the tables, the summary and the report.

#![doc(html_no_source)]

pub mod character;
pub mod model;
pub mod output;
pub mod spawns;
pub mod survey;
pub mod tally;
pub mod thumb;
pub mod world;

#[cfg(test)]
mod tests;
