//! `ACE.Database.Adapter` (shard side): the database row model to and from the entity model.
//! `WeenieConverter.cs` is mostly the world side (empyrean-content); its `ConvertToDatabaseBiota`, which
//! produces a shard biota, is [`weenie_converter`].

pub mod biota_converter;
pub mod biota_updater;
pub mod weenie_converter;

pub use biota_converter::BiotaConverter;
pub use biota_updater::BiotaUpdater;
pub use weenie_converter::WeenieConverter;
