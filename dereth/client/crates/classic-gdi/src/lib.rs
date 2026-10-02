//! The classic interface's two Windows graphics services, which the desktop host offers it: text
//! rasterised with the system's fonts, and printing an image.
//!
//! **Depends on** `dereth-classic-dat` (the font atlas format) and, on Windows, `windows-sys`.
//! **Used by** the desktop host (`dereth-desktop`), which hands the classic interface its fonts.
//!
//! **Must never** hold unsafe code outside its two Windows modules: they are in their own crate
//! because they need it, and the workspace forbids it everywhere else.
//!
//! * [`fonts`]: the classic interface draws its text with the system's fonts, the way the game
//!   of that era did: each font is asked for by height, width, weight and face, and every
//!   printable character of the Western code page is drawn once into a coverage atlas with its
//!   advance and bearings. The pixels are this machine's fonts, not recovered ones.
//! * [`print`](mod@print): the Help viewer's Print command, which hands a rendered page image to the
//!   printer the player picks in the system's print dialog.
//!
//! Off Windows both report that they are unavailable, and the classic client says so.

#![deny(unsafe_code)]

pub mod fonts;
pub mod print;
