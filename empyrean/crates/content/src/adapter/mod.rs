//! `Source/ACE.Database/Adapter` (World side). `BiotaConverter`/`BiotaUpdater` and
//! `WeenieConverter.ConvertToDatabaseBiota` produce Shard models and belong with empyrean-store.

pub mod weenie_converter;

pub use weenie_converter::convert_to_entity_weenie;
