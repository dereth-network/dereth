//! The classic interface's text rasterisation with system fonts, supplied by the desktop host.
//!
//! **Depends on** `dereth-classic-dat` (the font atlas format) and, on Windows, `windows-sys`.
//! **Used by** the desktop host (`dereth-desktop`), which hands the classic interface its fonts.
//!
//! **Must never** hold unsafe code outside its Windows font module: it is in its own crate
//! because it needs it, and the workspace forbids it everywhere else.
//!
//! * [`fonts`]: the classic interface draws its text with the system's fonts, the way the game
//!   of that era did: each font is asked for by height, width, weight and face, and every
//!   printable character of the Western code page is drawn once into a coverage atlas with its
//!   advance and bearings. The pixels are this machine's fonts, not recovered ones.
//!
//! Off Windows the font service reports that it is unavailable.

#![deny(unsafe_code)]

pub mod fonts;
