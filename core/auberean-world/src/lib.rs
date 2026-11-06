//! The world as data: the landscape's terrain and its looks, the region's sky, light and fog over
//! the day, and where placed objects stand and what they are made of.
//!
//! **Depends on** the records (`auberean-assets`, `auberean-dat`), `auberean-primitives`, and the
//! landscape core shared with Dereth: `dereth-landscape` (the grid and the block window),
//! `dereth-terrain` (vertices, splits, triangles and holes on any grid, the height table) and
//! `dereth-primitives`. **Used by** drawing (`auberean-draw`), physics (`auberean-physics`) and the
//! server (`yalain`), which need the same ground.
//!
//! - [`land`]: the landscape constants and the terrain rules built from them.
//! - [`terrain`]: a landblock's ground, normals, surfaces, colors and per-square layers.
//! - [`fog`]: the region's fog blocks blended over the day's light and fog.
//! - [`sky`]: the calendar, the day's light and fog, the sky's objects, lit terrain colors.
//! - [`calendar`]: a game time's year, season, day and part of the day, as the calendar names them.
//! - [`chargen`]: character generation's appearance choices, their values and the look they give.
//! - [`look`]: a visual description put together with every description it extends, and what
//!   its appearance does to each part group.
//! - [`place`]: a placed object resolved to meshes and transforms, as its look leaves them.
//! - [`scenery`]: the objects the landscape scatters over itself.
//! - [`sound`]: the places of a landblock that name a kind of ambient sound.
//! - [`water`]: which landblocks hold water, at what height and of what kind.
//! - [`records`]: the records, read from the data files the host opened and kept once decoded.
//! - [`wave`]: a waveform's value at a moment (particles, materials' moving pictures, lights'
//!   flicker), and [`noise`], the gradient noise some waveforms follow.
//! - [`random`], [`color`], [`math`]: the hashes, colors and arithmetic these share.
//!
//! Axes: x east, y north, z up, in meters; a landblock's frame has its origin at its south-west
//! corner.
//!
//! **Must never** do I/O, read a clock or draw: records come in through the traits the host
//! implements ([`terrain::LandSource`], [`look::LookRecords`], [`place::PlaceRecords`]), and what
//! leaves is data.

pub mod calendar;
pub mod chargen;
pub mod color;
pub mod fog;
pub mod inside;
pub mod land;
pub mod look;
pub mod math;
pub mod noise;
pub mod place;
pub mod property;
pub mod random;
pub mod records;
pub mod scenery;
pub mod sky;
pub mod sound;
pub mod terrain;
pub mod water;
pub mod wave;
