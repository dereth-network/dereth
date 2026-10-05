//! The pure retail game rules shared by the client and the server: values and decoded tables in,
//! numbers out.
//!
//! **Depends on** `dereth-primitives`, the decoded tables of `dereth-assets` and, behind the
//! default `proto` feature, `dereth-protocol`. **Used by** the client's object model
//! (`dereth-client-model`), animation
//! (`dereth-animation`), the retail UI (`dereth-ui-screens`), the SDK and the server
//! (`empyrean-world`).
//!
//! **Must never** hold the object model, read a file or touch the wire. The quality-reading
//! inquiries are written against [`quality::QualityRead`], which the object model implements.
//!
//! It holds the XP curves and attribute raise costs, the skill formula, burden and load, vendor
//! prices, the fellowship split, enchantment stacking ([`enchant`]), the name filter ([`taboo`]),
//! chess, the slot tables and the character-creation credits.

#![doc(html_no_source)]

pub mod advancement;
pub mod allegiance;
pub mod attributes;
pub mod burden;
/// A container's item and side-pack slot counts.
pub mod capacity;
/// Character-creation arithmetic: skill costs and credits.
pub mod chargen;
pub mod chess;
/// Combat rules: the elemental bonus against players.
pub mod combat;
#[cfg(feature = "proto")]
pub mod enchant;
pub mod fellowship;
/// The spell-formula rules: power levels and the scarab-only (foci) formula.
pub mod magic;
/// The movement system's run rate, jump height and jump stamina cost (one implementation).
pub mod movement;
pub mod names;
/// The property → `PublicWeenieDesc` mirror table.
pub mod pwd_mirror;
#[cfg(feature = "proto")]
pub mod quality;
pub mod skills;
pub mod slots;
pub mod taboo;
pub mod vendor;
pub mod weenie;
