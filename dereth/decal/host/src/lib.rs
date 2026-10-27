//! Dereth's plugins inside the retail client, as a library with a C interface that a Decal plugin loads.
//!
//! **Depends on** the client SDK (`dereth-client-sdk`), with its Luau and WebAssembly engines, and
//! the headless client (`dereth-headless`) for the plugin folders, stores and data folders kept in
//! files and the bus between clients on one machine, and the launcher's knowledge
//! (`dereth-launch`) for the plugins it installed. **Used by** the Decal plugin, which loads the
//! library built for `i686-pc-windows-msvc`; no crate depends on it.
//!
//! **Must never** read or write the retail client's memory or call into it: everything it does is
//! an op the Decal plugin carries out with Decal's own calls. It opens no window and no socket but
//! the bus's, and reads only the plugins' folders, the launcher's list of the plugins it installed,
//! its data folder and the game's data files.
//!
//! A Decal plugin (`dereth/decal/plugin`) loads this library into the retail client and hands it
//! what Decal hands plugins: every message the client receives and sends, the lines the player
//! types and the chat lines the client is about to show, the player's place and selection, and a
//! frame tick. The library keeps the same model of the game the Dereth client keeps, built from
//! those messages by the same session and object stream, runs the plugins in it with the same
//! plugin host and Luau engine, and answers with **ops**: what the Decal plugin is to do through
//! Decal's own calls (use an item, cast a spell, face a heading, show a window, mark an object, print
//! or hide a chat line). Nothing reads or writes the client's memory.
//!
//! | part | what |
//! |---|---|
//! | [`host`] | [`RetailHost`]: the model, the plugin host and a frame |
//! | [`ops`] | what the Decal plugin does, and which requests it can make at all |
//! | [`views`] | declared windows as Decal views: their layout, their controls' state, their events |
//! | [`marks`] | markers as Decal's 3-D objects |
//! | [`keys`] | the game's own keys, pressed for a plugin's attack, jump and held movement |
//! | [`ffi`] | the C interface |
//!
//! **What retail cannot do.** Decal reaches the game through the client's own interface: it can
//! use, apply, cast, move, drop, give, appraise and select, set the stance, face a heading and set
//! the run lock, trade, sell and buy at a vendor, salvage, train, and run a chat line through the
//! client's parser (which adds and removes friends). It cannot attack, split or merge a stack,
//! inscribe, make a fellowship, set a character option, close a vendor, hold a movement key, jump
//! or choose a character. A plugin's action of those kinds fails at once as `unavailable`
//! ([`ops::UNAVAILABLE`]), and the movements a journey needs are not offered (no `nav.journey` or
//! `nav.path`). With the player's leave, an attack, a jump and a held movement key are the game's
//! own keys, pressed while its window is in front ([`keys`]).

pub mod ffi;
pub mod host;
pub mod keys;
pub mod marks;
pub mod ops;
pub mod views;

pub use host::{RetailHost, Settings};
pub use ops::Op;
