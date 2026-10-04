//! The game's presentation rules: how an appraisal, the character sheet, the coordinates and the
//! target indicator are worded, numbered and placed, for any interface.
//!
//! **Depends on** `dereth-primitives`, the contract (`dereth-client-contract`), whose read-only
//! view the rules are functions of, and the shared rules' arithmetic (`dereth-rules`). **Used by**
//! the retail interface (`dereth-ui-screens`, which re-exports each module at its old path) and
//! the classic interface (`dereth-classic-ui`).
//!
//! **Must never** draw, lay out an element, hold a string table or reach the platform: these are
//! facts about what the game shows, and an interface decides where to show them. Like every core
//! crate it builds for `wasm32-unknown-unknown`.
//!
//! - [`appraisal`]: every line, row and colour an identify shows.
//! - [`character`]: the character information sheet's ladders, breakdowns and its augmentation
//!   and luminance section, composed over whatever string service the interface has.
//! - [`coordinates`]: the player's coordinates as text.
//! - [`journal`]: the journal's pages, its file's text and its captions.
//! - [`target`]: where the target indicator's brackets and arrows go.

pub mod appraisal;
pub mod character;
pub mod coordinates;
pub mod journal;
pub mod spell;
pub mod target;
