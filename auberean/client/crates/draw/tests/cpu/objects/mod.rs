//! Placed objects: vertex layouts, detail levels and mask turns, how they degrade with distance,
//! the walk through made-up cells, the lights effects place on objects and the dynamic objects'
//! volumes of shadow, and work shared among threads.

mod cull;
mod degrade;
mod layout;
mod lights;
mod volume;
mod workers;
