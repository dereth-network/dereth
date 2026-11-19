//! Horizon's windows of the game: the settings, the skill window, the examine window, the social
//! window, the map, the journal and a book read; the story's scenes over the whole screen; and
//! how the windows stack.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

mod book;
mod examine;
pub(crate) mod journal;
mod map;
mod map_picture;
mod plugins;
mod scale;
mod settings;
mod skills;
mod social;
mod stack;
mod story;
