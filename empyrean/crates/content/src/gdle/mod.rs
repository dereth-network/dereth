//! ACE.Adapter's GDLE and Lifestoned loaders: the rest of `Source/ACE.Adapter` that
//! `import-json` and `export-json` did not need.
//!
//! * [`loader`]: `GDLELoader`, every `TryLoad*`.
//! * [`lifestoned_loader`]: `LifestonedLoader`'s file and folder loaders.
//! * [`converter`]: the `GDLEConverter.TryConvert` overloads only the loaders call.
//! * [`models`]: the GDLE models only the loaders read.
//! * [`binders`]: the `JsonIgnore`d editor members of the GDLE models.
//! * [`lifestoned_enums`]: the Lifestoned.DataModel enum names the binders print (generated).
//! * [`json_kinds`]: the GDLE bulk documents `empyrean-import --json` accepts through these loaders.

pub mod binders;
pub mod converter;
pub mod json_kinds;
pub mod lifestoned_enums;
pub mod lifestoned_loader;
pub mod loader;
pub mod models;
