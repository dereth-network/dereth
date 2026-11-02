//! The game's sound: the sound manager every sound passes through, the rules of sound and music
//! settings, and the volume and positional laws, over the shared mixer.
//!
//! **Depends on** the shared mixer and its wave and MP3 decoders (`dereth-audio`), the sound and
//! music settings, waves, encoded tracks and the region's ambient sound table of `auberean-assets`,
//! the client's random numbers of `auberean-primitives`, and `dereth-primitives` for its
//! arithmetic. **Used by** the application's shell (`auberean-shell`), which hands the manager the
//! interface's and the world's requests, the listener and the places of the objects that make
//! sounds, and hands the host's device the mixer.
//!
//! **Must never** open a sound device or a data file: the host's device fills its blocks from
//! [`SoundPlayer::mixer`], and the records come through [`SoundFiles`].
//!
//! - [`request`]: a request to play ([`PlayRequest`]) and its choices;
//! - [`settings`]: what a sound setting (`.sif`) or a music setting (`.mif`) makes of a request;
//! - [`volume`]: the sliders and the level a sound plays at;
//! - [`space`]: where a placed sound is heard from, how far, and how it moves;
//! - [`records`]: the waves and encoded tracks, decoded to the mixer's sources;
//! - [`manager`]: the sound manager itself;
//! - [`options`]: the sound section of the client's preferences;
//! - [`music`]: music made of instruments, played on a performance through the mixer;
//! - [`ambient`]: ambient sound by place: what the places around the player make heard.

pub mod ambient;
pub mod manager;
pub mod music;
pub mod options;
pub mod records;
pub mod request;
pub mod settings;
pub mod space;
pub mod volume;

pub use manager::{effect_sound, Life, SoundPlayer, SoundStats};
pub use records::SoundFiles;
pub use request::{Channel, Looping, PathKind, PlayRequest, PlayType, Timing, Track};
pub use space::Listener;
pub use volume::ChannelVolumes;
