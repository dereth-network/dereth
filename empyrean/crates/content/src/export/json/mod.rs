// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/DeveloperContentCommands.cs
//! ACE's JSON export (`export-json`, `sql2json_*`): World models to the Lifestoned/GDLE JSON text
//! ACE writes with `JsonSerializer.Serialize(x, LifestonedConverter.SerializerSettings)`.
//!
//! * [`writer`]: `System.Text.Json` serialization with those settings.
//! * [`models`]: the ACE.Adapter GDLE/Lifestoned models, as they serialize.
//! * [`lifestoned`]: `LifestonedConverter.TryConvert(Weenie)`, `TryConvertACEWeenieToLSDJSON`,
//!   `LifestonedLoader.AppendMetadata`.
//! * [`gdle`]: `GDLEConverter.TryConvert` for quests, landblock instances, recipes and cookbooks.
//!
//! The file names and folders the commands write to belong to the commands. ACE's
//! `DateTime.UtcNow` is a `now` parameter.

mod escape_table;
pub mod gdle;
pub mod lifestoned;
pub mod models;
pub mod writer;

pub use gdle::{try_convert_cookbooks, try_convert_landblock, try_convert_quest};
pub use lifestoned::{append_metadata, serialize, try_convert_ace_weenie_to_lsd_json};
pub use models::{Landblock, LsdWeenie, Quest as GdleQuest, RecipeCombined};
pub use writer::{SerializeError, ToJson, NEW_LINE};
