//! `Source/ACE.Entity/Models`: [`Weenie`] (a world-database template), [`Biota`] (one object's
//! property state), their property records and their extension methods. Pure data: nothing here
//! knows about a database.
//!
//! # Property bags
//! ACE's `Dictionary<PropertyInt, int>` and friends are [`DotNetDict`](empyrean_common::dotnet::DotNetDict)s,
//! because their enumeration order (insertion order, with removed slots reused most recent first)
//! reaches the wire. ACE leaves an empty collection `null` to save memory; here that is `None`.
//! Records that are classes in ACE (`PropertiesSkill`, `PropertiesPosition`, ...) are plain
//! structs. Their Rust `Clone` copies every field; ACE's `Clone()` method, which drops
//! `DatabaseRecordId` on some records, is `ace_clone()`.
//!
//! # Weenie to biota aliasing
//! [`convert_to_biota`](crate::adapter::convert_to_biota) with
//! `referenceWeenieCollectionsForCommonProperties` makes ACE's new biota point at the cached
//! weenie's own create list, emotes, event filter, generator list and body parts. ACE's comment
//! on those collections says they "typically aren't modified over the original weenie", and any
//! code that did modify one through the biota would silently change the weenie for every later
//! object. Here those five collections are `Option<Arc<...>>` on both [`Weenie`] and [`Biota`]:
//!
//! * conversion clones the `Arc`, so the biota reads exactly the weenie's values, with no copy;
//! * a write goes through `Biota::properties_emote_mut()` and its siblings, which call
//!   `Arc::make_mut` and so copy the collection the first time a shared one is written;
//! * replacing the whole collection (`biota.properties_emote = ...`) just drops the biota's
//!   reference, as assigning the property does in ACE.
//!
//! A biota mutation therefore never changes the cached weenie, and nothing is copied until a biota
//! actually writes. The copy is a Rust `Clone` of the collection (all fields and free slots kept),
//! i.e. the state the shared collection would have had at that moment.
//!
//! # Locks
//! ACE's extension methods take the owning object's `ReaderWriterLockSlim`. The world owns each
//! biota from a single thread, and `&`/`&mut` borrows give the same
//! exclusion statically, so the parameter is dropped everywhere. Methods that return a record
//! return a reference into the biota where ACE callers may keep using it.

pub mod biota;
pub mod biota_extensions;
pub mod i_weenie;
pub mod properties_allegiance;
pub mod properties_allegiance_extensions;
pub mod properties_anim_part;
pub mod properties_anim_part_extensions;
pub mod properties_attribute;
pub mod properties_attribute_2nd;
pub mod properties_body_part;
pub mod properties_book;
pub mod properties_book_page_data;
pub mod properties_book_page_data_extensions;
pub mod properties_create_list;
pub mod properties_emote;
pub mod properties_emote_action;
pub mod properties_enchantment_registry;
pub mod properties_enchantment_registry_extensions;
pub mod properties_generator;
pub mod properties_palette;
pub mod properties_palette_extensions;
pub mod properties_position;
pub mod properties_skill;
pub mod properties_texture_map;
pub mod properties_texture_map_extensions;
pub mod weenie;
pub mod weenie_extensions;

pub use biota::Biota;
pub use i_weenie::{IWeenie, PropertyKey};
pub use properties_allegiance::PropertiesAllegiance;
pub use properties_anim_part::PropertiesAnimPart;
pub use properties_attribute::PropertiesAttribute;
pub use properties_attribute_2nd::PropertiesAttribute2nd;
pub use properties_body_part::PropertiesBodyPart;
pub use properties_book::PropertiesBook;
pub use properties_book_page_data::PropertiesBookPageData;
pub use properties_create_list::PropertiesCreateList;
pub use properties_emote::PropertiesEmote;
pub use properties_emote_action::PropertiesEmoteAction;
pub use properties_enchantment_registry::PropertiesEnchantmentRegistry;
pub use properties_generator::PropertiesGenerator;
pub use properties_palette::PropertiesPalette;
pub use properties_position::PropertiesPosition;
pub use properties_skill::PropertiesSkill;
pub use properties_texture_map::PropertiesTextureMap;
pub use weenie::Weenie;
