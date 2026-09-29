//! The port of ACE's `Source/ACE.Entity`: the generated enums with their attribute tables, and the
//! entity models.
//!
//! **Depends on** `empyrean-common` and the shared `dereth-primitives` and `dereth-protocol`.
//! **Used by** every server crate above it: `empyrean-tables`, `empyrean-content`,
//! `empyrean-store`, `empyrean-dat`, `empyrean-world`, `empyrean-command`, `empyrean-server`,
//! `empyrean-import`'s tests and `empyrean-testkit`.
//!
//! **Must never** depend on a server crate other than `empyrean-common`, so that every crate above
//! it can name its types, or on a client-only (`dereth/`) crate (`cargo xtask separation`).
//!
//! One module per ACE file (snake case). `numerics` (the `System.Numerics` subset) and `binary_io`
//! (the `BinaryReader` subset) are .NET models the ported files need, not ACE code; they live in
//! `empyrean_common::dotnet` and are re-exported here under their old paths.

pub mod adapter;
pub mod enums;
pub mod models;
pub use empyrean_common::dotnet::numerics;
pub mod object_guid;
pub mod packet_op_code_names;

pub mod anim_data;
pub mod animation_part_change;
pub mod appearance;
pub mod attack_frame_params;
pub use empyrean_common::dotnet::binary_reader as binary_io;
pub mod character_create_info;
pub mod character_position_extensions;
pub mod create_profile;
pub mod emote;
pub mod emote_set;
pub mod frame;
pub mod generator_queue_node;
pub mod generator_registry_node;
pub mod generic_property_id;
pub mod landblock_id;
pub mod linq_extensions;
pub mod obj_desc;
pub mod page_data;
pub mod position;
pub mod shared_types;
pub mod spell_bar_positions;
pub mod sub_palette;
pub mod sw_vertex;
pub mod texture_map_change;

pub use adapter::convert_to_biota;
pub use anim_data::AnimData;
pub use animation_part_change::AnimationPartChange;
pub use appearance::Appearance;
pub use attack_frame_params::AttackFrameParams;
pub use binary_io::BinaryReader;
pub use character_create_info::CharacterCreateInfo;
pub use create_profile::CreateProfile;
pub use emote::Emote;
pub use emote_set::EmoteSet;
pub use frame::Frame;
pub use generator_queue_node::GeneratorQueueNode;
pub use generator_registry_node::GeneratorRegistryNode;
pub use generic_property_id::GenericPropertyId;
pub use landblock_id::LandblockId;
pub use models::{Biota, IWeenie, PropertyKey, Weenie};
pub use numerics::{Quaternion, Vector2, Vector3};
pub use obj_desc::ObjDesc;
pub use object_guid::{GuidType, ObjectGuid};
pub use page_data::PageData;
pub use position::{BadCoordinates, Position};
pub use spell_bar_positions::SpellBarPositions;
pub use sub_palette::SubPalette;
pub use texture_map_change::TextureMapChange;
